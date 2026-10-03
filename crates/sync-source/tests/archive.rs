//! A build read from an archive of past builds, against the published history. Ignored by default:
//! it needs both, and neither is in this repository.
//!
//! - `CENSUS_SYNC_ARCHIVE`: the archive's root, holding `game-win/<MANIFEST ID>.manifest` and the
//!   merged bundle `lol.bundle`.
//! - `CENSUS_SYNC_HISTORY`: a bare clone of the history.
//! - `CENSUS_SYNC_COMMIT`: the build's commit, `history` by default.
//!
//! `cargo test -p sync-source --test archive -- --ignored`

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

use camino::Utf8PathBuf;
use sync_format::{entry_files, kind_of, wad_dir, wad_yaml, KIND_LINK};
use sync_source::{entry_bytes, read_wad_table, FileReader, Manifest, MergedBundle};

/// The WADs read entry by entry.
const WADS: [&str; 2] = ["DATA/FINAL/Champions/Ahri.wad.client", "DATA/FINAL/Champions/Ahri.en_US.wad.client"];

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set; see this file's header"))
}

fn git(args: &[&str]) -> String {
    let out = Command::new("git").arg(format!("--git-dir={}", env("CENSUS_SYNC_HISTORY"))).args(args).output().expect("git runs");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("git prints UTF-8")
}

/// Blobs' texts by id, in the order asked.
fn cat(ids: &[String]) -> Vec<String> {
    let mut child = Command::new("git")
        .arg(format!("--git-dir={}", env("CENSUS_SYNC_HISTORY")))
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("git cat-file starts");
    let mut stdin = child.stdin.take().expect("cat-file's stdin");
    let request: String = ids.iter().map(|id| format!("{id}\n")).collect();
    let writer = std::thread::spawn(move || stdin.write_all(request.as_bytes()));
    let mut stdout = BufReader::new(child.stdout.take().expect("cat-file's stdout"));
    let texts = ids
        .iter()
        .map(|id| {
            let mut header = String::new();
            stdout.read_line(&mut header).expect("a cat-file header");
            let size: usize = header.split_whitespace().nth(2).and_then(|s| s.parse().ok()).unwrap_or_else(|| panic!("{id}: {header:?}"));
            let mut buf = vec![0u8; size + 1];
            stdout.read_exact(&mut buf).expect("a cat-file object");
            buf.pop();
            String::from_utf8(buf).expect("a UTF-8 blob")
        })
        .collect();
    writer.join().expect("the cat-file writer").expect("cat-file takes the requests");
    child.wait().expect("cat-file exits");
    texts
}

struct Build {
    commit: String,
    manifest: Manifest,
    bundle: MergedBundle,
    /// Every file of the commit under `files/`: path to blob id.
    tree: BTreeMap<String, String>,
}

fn open() -> Build {
    let commit = std::env::var("CENSUS_SYNC_COMMIT").unwrap_or_else(|_| "history".to_string());
    let message = git(&["log", "-1", "--format=%B", &commit]);
    let id = message.lines().find_map(|l| l.strip_prefix("Census-Manifest: ")).expect("a Census-Manifest trailer");
    let root = Utf8PathBuf::from(env("CENSUS_SYNC_ARCHIVE"));
    let manifest = Manifest::read(&root.join("game-win").join(format!("{}.manifest", id.to_uppercase()))).expect("the manifest");
    let bundle = MergedBundle::open(&root.join("lol.bundle")).expect("the merged bundle");
    let tree = git(&["ls-tree", "-r", "--format=%(objectname) %(path)", &commit, "--", "files/"])
        .lines()
        .map(|l| {
            let (id, path) = l.split_once(' ').expect("an ls-tree line");
            (path.to_string(), id.to_string())
        })
        .collect();
    Build { commit, manifest, bundle, tree }
}

fn field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(": ")).map(|v| v.trim_matches('"').to_string())
}

#[test]
#[ignore = "needs CENSUS_SYNC_ARCHIVE and CENSUS_SYNC_HISTORY"]
fn every_wad_of_the_build_has_its_published_wad_yaml() {
    let build = open();
    let wads: Vec<_> = build.manifest.files.iter().filter(|f| f.path.ends_with(".wad.client")).collect();
    let paths: Vec<String> = wads.iter().map(|f| format!("{}/_wad.yaml", wad_dir(&f.path))).collect();
    let published: Vec<&str> = build.tree.keys().filter(|p| p.ends_with("/_wad.yaml")).map(String::as_str).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(sorted, published, "the manifest's WADs and the commit's");
    let texts = cat(&paths.iter().map(|p| build.tree[p].clone()).collect::<Vec<_>>());
    let mut failed = 0;
    for ((file, path), published) in wads.iter().zip(&paths).zip(&texts) {
        let chunks = build.manifest.chunks_of(file).expect("the file's chunks");
        let mut reader = FileReader::new(&build.bundle, chunks).expect("a reader");
        let (header, _) = read_wad_table(&mut reader).unwrap_or_else(|e| panic!("{}: {e}", file.path));
        let rendered = wad_yaml(header.major, header.minor, file.id, file.tags.iter().map(String::as_str));
        if &rendered != published {
            eprintln!("{path}: rendered\n{rendered}published\n{published}");
            failed += 1;
        }
    }
    let mut versions: Vec<u8> = build.manifest.params.iter().map(|p| p.version).collect();
    versions.sort_unstable();
    versions.dedup();
    eprintln!("{} WADs of {}, chunking versions {versions:?}", wads.len(), build.commit);
    assert_eq!(failed, 0, "of {} WADs", wads.len());
}

#[test]
#[ignore = "needs CENSUS_SYNC_ARCHIVE and CENSUS_SYNC_HISTORY"]
fn every_entry_of_some_wads_reads_and_renders_as_published() {
    let build = open();
    let legacy = cat(&[git(&["rev-parse", &format!("{}:build.yaml", build.commit)]).trim().to_string()])[0].contains("\nlegacyBins: true\n");
    let (mut entries, mut files, mut failed) = (0, 0, 0);
    for path in WADS {
        let file = build.manifest.files.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("{path} is not in the manifest"));
        let dir = wad_dir(path);
        let mut reader = FileReader::new(&build.bundle, build.manifest.chunks_of(file).expect("chunks")).expect("a reader");
        let (_, table) = read_wad_table(&mut reader).expect("the table");
        // A path hash the table lists twice takes its first entry's bytes and its last checksum.
        let mut first: Vec<_> = Vec::new();
        let mut checksum: HashMap<u64, Option<u64>> = HashMap::new();
        for e in &table {
            if checksum.insert(e.path_hash, e.checksum).is_none() {
                first.push(*e);
            }
        }
        let published: Vec<(&String, &String)> = build.tree.range(format!("{dir}/")..format!("{dir}0")).collect();
        let texts: HashMap<&String, String> =
            published.iter().map(|(p, _)| *p).zip(cat(&published.iter().map(|(_, id)| id.to_string()).collect::<Vec<_>>())).collect();
        let mut rendered: BTreeMap<String, String> = BTreeMap::new();
        for e in &first {
            let bytes = entry_bytes(&reader.read_range(e.offset, e.stored_size).expect("the stored bytes"), e).expect("the bytes");
            let kind = if e.is_link() { KIND_LINK } else { kind_of(&bytes) };
            for (name, text) in entry_files(e.path_hash, checksum[&e.path_hash], kind, &bytes, legacy).files {
                rendered.insert(format!("{dir}/{name}"), text);
            }
        }
        let published: BTreeMap<&String, &String> = texts.iter().filter(|(p, _)| !p.ends_with("/_wad.yaml")).map(|(p, t)| (*p, t)).collect();
        if !published.keys().map(|p| p.as_str()).eq(rendered.keys().map(String::as_str)) {
            eprintln!("{path}: {} files published, {} rendered", published.len(), rendered.len());
            failed += 1;
        }
        for (p, text) in &published {
            if rendered.get(p.as_str()).is_some_and(|ours| ours != *text) {
                eprintln!("{p} differs; published:\n{}", text.lines().take(12).collect::<Vec<_>>().join("\n"));
                failed += 1;
            }
        }
        let own = published.keys().filter(|p| p.ends_with(".yaml")).count();
        for e in &first {
            let yaml = &texts[&format!("{dir}/{:02x}/{:016x}.yaml", e.path_hash >> 56, e.path_hash)];
            assert_eq!(field(yaml, "checksum"), checksum[&e.path_hash].map(|c| format!("{c:016x}")), "{path} {:016x}", e.path_hash);
        }
        eprintln!("{path}: {} entries, {own} entry files, {} files", first.len(), published.len());
        entries += first.len();
        files += published.len();
    }
    assert_eq!(failed, 0, "of {entries} entries, {files} files");
}
