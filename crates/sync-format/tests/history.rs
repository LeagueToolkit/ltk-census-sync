//! The writers against the published history (`docs/TOOLING.md`, "Tests"). Ignored by default:
//! they need a clone of the history and, for entries, the entries' bytes, neither of which is in
//! this repository.
//!
//! - `CENSUS_SYNC_HISTORY`: a bare clone of the history.
//! - `CENSUS_SYNC_SAMPLES`: entry bytes, each file named for its SHA-256, and `entries.txt`, one
//!   `<commit> <path of the entry's own file>` per line.
//!
//! `cargo test -p sync-format --test history -- --ignored`

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};
use sync_format::{
    build_yaml, commit_message, commit_time, entry_files, kind_of, wad_yaml, BuildFacts, Rads, AUTHOR, CENSUS_YAML,
    GITATTRIBUTES, KIND_LINK, MAIN_LICENSE, MAIN_README, SCHEMAS,
};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set; see this file's header"))
}

struct History {
    dir: String,
}

impl History {
    fn open() -> Self {
        Self { dir: env("CENSUS_SYNC_HISTORY") }
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git").arg(format!("--git-dir={}", self.dir)).args(args).output().expect("git runs");
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).expect("git prints UTF-8")
    }

    /// Objects' contents by name (`<id>`, `<commit>:<path>`), in the order asked.
    fn cat(&self, names: &[String]) -> Vec<String> {
        let mut child = Command::new("git")
            .arg(format!("--git-dir={}", self.dir))
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("git cat-file starts");
        let mut stdin = child.stdin.take().expect("cat-file's stdin");
        let request: String = names.iter().map(|n| format!("{n}\n")).collect();
        let writer = std::thread::spawn(move || stdin.write_all(request.as_bytes()));
        let mut stdout = BufReader::new(child.stdout.take().expect("cat-file's stdout"));
        let mut texts = Vec::with_capacity(names.len());
        for name in names {
            let mut header = String::new();
            stdout.read_line(&mut header).expect("a cat-file header");
            let size: usize = header
                .split_whitespace()
                .nth(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| panic!("{name}: cat-file answered {header:?}"));
            let mut buf = vec![0u8; size + 1];
            stdout.read_exact(&mut buf).expect("a cat-file object");
            buf.pop();
            texts.push(String::from_utf8(buf).unwrap_or_else(|_| panic!("{name} is not UTF-8")));
        }
        writer.join().expect("the cat-file writer").expect("cat-file takes the requests");
        child.wait().expect("cat-file exits");
        texts
    }

    /// Every file under a path of a commit: path to blob id.
    fn files(&self, commit: &str, path: &str) -> BTreeMap<String, String> {
        self.git(&["ls-tree", "-r", "--format=%(objectname) %(path)", commit, "--", path])
            .lines()
            .map(|line| {
                let (id, path) = line.split_once(' ').expect("an ls-tree line");
                (path.to_string(), id.to_string())
            })
            .collect()
    }
}

/// Whether two texts are equal; when not, the lines around the first difference, as a diff.
fn same(what: &str, published: &str, rendered: &str) -> bool {
    if published == rendered {
        return true;
    }
    let (p, r): (Vec<&str>, Vec<&str>) = (published.split_inclusive('\n').collect(), rendered.split_inclusive('\n').collect());
    let first = p.iter().zip(&r).position(|(a, b)| a != b).unwrap_or(p.len().min(r.len()));
    let window = |lines: &[&str]| lines[first.saturating_sub(3).min(lines.len())..(first + 8).min(lines.len())].concat();
    let (p, r) = (window(&p), window(&r));
    eprintln!("{what}: differs at line {}\n{}", first + 1, pretty_assertions::StrComparison::new(&p, &r));
    false
}

fn unquote(value: &str) -> String {
    let inner = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or_else(|| panic!("{value} is not quoted"));
    assert!(!inner.contains('\\'), "{value} has an escape");
    inner.to_string()
}

/// A field of a YAML file's top level, unquoted.
fn top(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(": ")).map(unquote)
}

/// What a published `build.yaml` says.
fn build_facts(text: &str) -> BuildFacts {
    let field = |key: &str| top(text, key).unwrap_or_else(|| panic!("build.yaml without {key}"));
    BuildFacts {
        version: field("version"),
        patch: field("patch"),
        manifest: u64::from_str_radix(&field("manifest"), 16).expect("a hex manifest"),
        rads: (field("source") == "rads").then(|| Rads { solution: field("solution"), release: field("release"), exe: field("exe") }),
        date: field("date"),
        legacy_bins: text.contains("\nlegacyBins: true\n"),
        realms: text.lines().filter_map(|l| l.strip_prefix(" - ")).map(unquote).collect(),
    }
}

#[test]
#[ignore = "needs CENSUS_SYNC_HISTORY"]
fn every_commit_has_the_build_yaml_message_and_time_of_its_build() {
    let history = History::open();
    let commits: Vec<String> = history.git(&["rev-list", "--reverse", "history-v2"]).lines().map(str::to_string).collect();
    let builds = history.cat(&commits.iter().map(|c| format!("{c}:build.yaml")).collect::<Vec<_>>());
    let objects = history.cat(&commits);
    let mut failed = 0;
    for ((commit, build), object) in commits.iter().zip(&builds).zip(&objects) {
        let facts = build_facts(build);
        let (headers, message) = object.split_once("\n\n").expect("a commit with a message");
        let time = commit_time(&facts.date).unwrap_or_else(|| panic!("{commit}: date {}", facts.date));
        let signature = format!("{AUTHOR} {time} +0000");
        let signed = headers.contains(&format!("\nauthor {signature}\ncommitter {signature}"));
        if !signed {
            eprintln!("{commit}: not signed `{signature}`:\n{headers}");
        }
        let ok = same(&format!("{commit} build.yaml"), build, &build_yaml(&facts))
            & same(&format!("{commit} message"), message, &commit_message(&facts))
            & signed;
        failed += usize::from(!ok);
    }
    assert_eq!(failed, 0, "of {} commits", commits.len());
}

#[test]
#[ignore = "needs CENSUS_SYNC_HISTORY"]
fn the_fixed_texts_are_the_published_ones() {
    let history = History::open();
    let mut names =
        vec!["history-v2:.gitattributes".to_string(), "history-v2:census.yaml".to_string(), "main:README.md".to_string(), "main:LICENSE".to_string()];
    names.extend(SCHEMAS.iter().map(|(path, _)| format!("history-v2:{path}")));
    let published = history.cat(&names);
    let mut ours = vec![GITATTRIBUTES, CENSUS_YAML, MAIN_README, MAIN_LICENSE];
    ours.extend(SCHEMAS.iter().map(|(_, text)| *text));
    let mut failed = 0;
    for ((name, published), ours) in names.iter().zip(&published).zip(ours) {
        failed += usize::from(!same(name, published, ours));
    }
    assert_eq!(failed, 0);
}

#[test]
#[ignore = "needs CENSUS_SYNC_HISTORY"]
fn every_wad_yaml_of_the_tip_renders_from_its_fields() {
    let history = History::open();
    let wads: Vec<String> = history.files("history-v2", "files/").into_iter().filter(|(p, _)| p.ends_with("/_wad.yaml")).map(|(_, id)| id).collect();
    let mut failed = 0;
    for (id, text) in wads.iter().zip(history.cat(&wads)) {
        let version = top(&text, "version").expect("a version");
        let (major, minor) = version.split_once('.').expect("major.minor");
        let file_id = u64::from_str_radix(&top(&text, "fileId").expect("a file id"), 16).expect("a hex file id");
        let tags: Vec<String> = text.lines().filter_map(|l| l.strip_prefix(" - ")).map(unquote).collect();
        let rendered = wad_yaml(major.parse().expect("a major"), minor.parse().expect("a minor"), file_id, tags.iter().map(String::as_str));
        failed += usize::from(!same(id, &text, &rendered));
    }
    assert_eq!(failed, 0, "of {} WADs", wads.len());
}

#[test]
#[ignore = "needs CENSUS_SYNC_HISTORY and CENSUS_SYNC_SAMPLES"]
fn sample_entries_render_as_published() {
    let history = History::open();
    let samples = env("CENSUS_SYNC_SAMPLES");
    let list = fs_err::read_to_string(Path::new(&samples).join("entries.txt")).expect("entries.txt");
    let (mut entries, mut files, mut failed) = (0, 0, 0);
    for line in list.lines().filter(|l| !l.is_empty()) {
        let (commit, own) = line.split_once(' ').expect("`<commit> <path>`");
        let (dir, name) = own.rsplit_once('/').and_then(|(d, n)| Some((d.rsplit_once('/')?.0, n))).expect("<dir>/xx/<hash>.yaml");
        let path_hash = u64::from_str_radix(name.trim_end_matches(".yaml"), 16).expect("a hex path hash");
        let build = history.cat(&[format!("{commit}:build.yaml")]).remove(0);
        let legacy = build_facts(&build).legacy_bins;

        let mut listing = history.files(commit, &format!("{}.bin/", own.trim_end_matches(".yaml")));
        listing.insert(own.to_string(), history.git(&["rev-parse", &format!("{commit}:{own}")]).trim().to_string());
        let texts = history.cat(&listing.values().cloned().collect::<Vec<_>>());
        let published: BTreeMap<String, String> = listing.into_keys().zip(texts).collect();

        let yaml = &published[own];
        let sha = top(yaml, "sha256").expect("a sha256");
        let bytes = fs_err::read(Path::new(&samples).join(&sha)).expect("the entry's bytes");
        assert_eq!(hex(&Sha256::digest(&bytes)), sha, "{own}: the sample's bytes");
        let checksum = top(yaml, "checksum").map(|c| u64::from_str_radix(&c, 16).expect("a hex checksum"));
        let kind = top(yaml, "kind").unwrap_or_default();
        if kind != KIND_LINK {
            assert_eq!(kind_of(&bytes), kind, "{own}: the kind");
        }

        let rendered = entry_files(path_hash, checksum, &kind, &bytes, legacy);
        let rendered: BTreeMap<String, String> = rendered.files.into_iter().map(|(p, t)| (format!("{dir}/{p}"), t)).collect();
        let mut ok = published.keys().eq(rendered.keys());
        if !ok {
            let missing = published.keys().filter(|p| !rendered.contains_key(*p)).count();
            let extra = rendered.keys().filter(|p| !published.contains_key(*p)).count();
            eprintln!("{own}: {missing} published files not rendered, {extra} rendered files not published");
        }
        for (path, text) in &published {
            if let Some(ours) = rendered.get(path) {
                ok &= same(path, text, ours);
            }
        }
        entries += 1;
        files += published.len();
        failed += usize::from(!ok);
    }
    eprintln!("{entries} entries, {files} files");
    assert_eq!(failed, 0, "of {entries} entries");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
