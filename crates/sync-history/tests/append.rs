//! An append onto a small history the test writes, in a scratch repository.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::process::Command;

use camino::Utf8PathBuf;
use ltk_wad::{WadChunk, WadChunkCompression, WadHash};
use sync_format::{build_yaml, commit_message, entry_files, wad_yaml, BuildFacts};
use sync_history::{append, Error, Git};
use sync_source::{BundleChunk, ChunkHash, ChunkRef, ChunkSource, ChunkingParams, Manifest, ManifestFile};

const KEPT: u64 = 0x1111_1111_1111_1111;
const REREAD: u64 = 0x2222_2222_2222_2222;
const GONE: u64 = 0x3333_3333_3333_3333;
const NEW: u64 = 0x4444_4444_4444_4444;

/// Frames by chunk id.
#[derive(Default)]
struct Frames(HashMap<u64, Vec<u8>>);

impl ChunkSource for Frames {
    fn frames(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, sync_source::Error> {
        wanted.iter().map(|c| self.0.get(&c.id).cloned().ok_or(sync_source::Error::MissingChunk(c.id))).collect()
    }
}

/// A v3.4 WAD of uncompressed entries: (path hash, checksum, bytes).
fn wad(entries: &[(u64, u64, &[u8])]) -> Vec<u8> {
    let mut bytes = b"RW\x03\x04".to_vec();
    bytes.extend([0u8; 264]);
    bytes.extend((entries.len() as i32).to_le_bytes());
    let mut offset = bytes.len() + 32 * entries.len();
    for (hash, checksum, data) in entries {
        let chunk = WadChunk {
            path_hash: WadHash(*hash),
            data_offset: offset,
            compressed_size: data.len(),
            uncompressed_size: data.len(),
            compression_type: WadChunkCompression::None,
            is_duplicated: false,
            frame_count: 0,
            start_frame: 0,
            checksum: *checksum,
        };
        chunk.write_v3_4(&mut bytes).unwrap();
        offset += data.len();
    }
    for (_, _, data) in entries {
        bytes.extend(*data);
    }
    bytes
}

/// A manifest of WADs, each one chunk, and the frames of those whose bytes are given.
fn manifest(wads: &[(&str, u64, Vec<u8>, bool)]) -> (Manifest, Frames) {
    let (mut files, mut places, mut frames) = (Vec::new(), HashMap::new(), Frames::default());
    for (path, id, bytes, available) in wads {
        let chunk = ChunkHash::Blake3.id_of(bytes);
        places.insert(chunk, BundleChunk { bundle: 1, offset: 0, compressed_size: 0, uncompressed_size: bytes.len() as u32 });
        if *available {
            frames.0.insert(chunk, zstd::bulk::compress(bytes, 1).unwrap());
        }
        files.push(ManifestFile { id: *id, path: path.to_string(), size: bytes.len() as u64, tags: Vec::new(), params_id: 1, chunk_ids: vec![chunk] });
    }
    (Manifest::new(0xabc, vec![ChunkingParams { id: 1, version: 4 }], files, places), frames)
}

fn own(dir: &str, hash: u64) -> String {
    format!("{dir}/{:02x}/{hash:016x}.yaml", hash >> 56)
}

fn checksum_yaml(checksum: u64) -> String {
    format!("sha256: \"{}\"\nchecksum: \"{checksum:016x}\"\n", "00".repeat(32))
}

struct Scratch {
    _dir: tempfile::TempDir,
    git: Git,
}

/// A bare repository whose `history` is one commit of these files.
fn history(files: &[(String, String)]) -> Scratch {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("history.git")).unwrap();
    assert!(Command::new("git").args(["init", "--bare", "-q", path.as_str()]).status().unwrap().success());
    let git = Git::open(&path).unwrap();
    let mut import = git.fast_import().unwrap();
    let out = import.stream();
    write!(out, "commit refs/heads/history\ncommitter census <census@localhost> 0 +0000\ndata 5\nbase\n").unwrap();
    for (path, text) in files {
        write!(out, "M 100644 inline {path}\ndata {}\n{text}\n", text.len()).unwrap();
    }
    import.finish().unwrap();
    Scratch { _dir: dir, git }
}

fn tree(git: &Git, rev: &str) -> BTreeMap<String, String> {
    git.run(&["ls-tree", "-r", "--format=%(path) %(objectname)", rev])
        .unwrap()
        .lines()
        .map(|l| l.split_once(' ').map(|(p, b)| (p.to_string(), b.to_string())).unwrap())
        .collect()
}

fn facts() -> BuildFacts {
    BuildFacts {
        version: "16.20.1".into(),
        patch: "16.20".into(),
        manifest: 0xabc,
        rads: None,
        date: "2026-10-01".into(),
        legacy_bins: false,
        realms: vec!["NA1".into(), "EUW1".into()],
    }
}

#[test]
fn an_append_writes_only_what_the_build_changed() {
    let (a, b, c, d) = ("files/data/a.wad.client", "files/data/b.wad.client", "files/data/c.wad.client", "files/data/d.wad.client");
    let rito = format!("{b}/22/{REREAD:016x}.bin/b0/b08673e8.rito");
    let scratch = history(&[
        ("build.yaml".into(), "version: \"16.19.1\"\n".into()),
        ("census.yaml".into(), "format: 2\n".into()),
        (format!("{a}/_wad.yaml"), wad_yaml(3, 4, 0xA1, [])),
        (own(a, KEPT), checksum_yaml(1)),
        (format!("{b}/_wad.yaml"), wad_yaml(3, 4, 0xB1, [])),
        (own(b, KEPT), checksum_yaml(5)),
        (own(b, REREAD), checksum_yaml(6)),
        (rito.clone(), "a bin entry\n".into()),
        (own(b, GONE), checksum_yaml(7)),
        (format!("{c}/_wad.yaml"), wad_yaml(3, 4, 0xC1, [])),
        (own(c, KEPT), checksum_yaml(8)),
    ]);
    let git = &scratch.git;
    let before = tree(git, "history");
    let tip = git.rev_parse("history").unwrap();

    // A carries over, and its bytes are not there to read; B keeps one entry, reads one, loses one
    // and gains one; C is gone; D is new.
    let (manifest, frames) = manifest(&[
        ("DATA/A.wad.client", 0xA1, wad(&[(KEPT, 1, b"a")]), false),
        ("DATA/B.wad.client", 0xB2, wad(&[(KEPT, 5, b"kept"), (REREAD, 9, b"PTCH\x01\0\0\0"), (NEW, 10, b"new")]), true),
        ("DATA/D.wad.client", 0xD1, wad(&[(NEW, 11, b"d")]), true),
    ]);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
    let appended = append(git, "history", &facts(), &manifest, &frames, &pool).unwrap();
    assert_eq!((appended.wads, appended.changed, appended.read), (3, 2, 3));

    let after = tree(git, "history");
    let text = |path: &str| git.run(&["cat-file", "blob", &format!("history:{path}")]).unwrap();
    let rendered = |hash, checksum, kind, bytes: &[u8]| entry_files(hash, Some(checksum), kind, bytes, false).files.remove(0).1;
    let mut paths: Vec<&str> = after.keys().map(String::as_str).collect();
    paths.sort_unstable();
    let mut expected = vec![
        "build.yaml".to_string(),
        "census.yaml".into(),
        format!("{a}/_wad.yaml"),
        own(a, KEPT),
        format!("{b}/_wad.yaml"),
        own(b, KEPT),
        own(b, REREAD),
        own(b, NEW),
        format!("{d}/_wad.yaml"),
        own(d, NEW),
    ];
    expected.sort_unstable();
    assert_eq!(paths, expected);
    for kept in [format!("{a}/_wad.yaml"), own(a, KEPT), own(b, KEPT), "census.yaml".into()] {
        assert_eq!(after[&kept], before[&kept], "{kept} keeps its blob");
    }
    assert_eq!(text("build.yaml"), build_yaml(&facts()));
    assert_eq!(text(&format!("{b}/_wad.yaml")), wad_yaml(3, 4, 0xB2, []));
    assert_eq!(text(&own(b, REREAD)), rendered(REREAD, 9, "bin", b"PTCH\x01\0\0\0"));
    assert_eq!(text(&own(b, NEW)), rendered(NEW, 10, "", b"new"));
    assert_eq!(text(&own(d, NEW)), rendered(NEW, 11, "", b"d"));

    let commit = git.run(&["cat-file", "commit", "history"]).unwrap();
    let signature = "census <census@localhost> 1790812800 +0000";
    assert!(commit.contains(&format!("\nparent {tip}\nauthor {signature}\ncommitter {signature}\n\n")), "{commit}");
    assert!(commit.ends_with(&commit_message(&facts())));
    assert_eq!(appended.commit, git.rev_parse("history").unwrap());
}

#[test]
fn an_append_that_stops_leaves_the_branch_where_it_was() {
    let dir = "files/data/b.wad.client";
    let scratch = history(&[(format!("{dir}/_wad.yaml"), wad_yaml(3, 4, 0xB1, [])), (own(dir, KEPT), checksum_yaml(5))]);
    let git = &scratch.git;
    let tip = git.rev_parse("history").unwrap();
    let (manifest, frames) = manifest(&[("DATA/B.wad.client", 0xB2, wad(&[(KEPT, 6, b"x")]), false)]);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
    let result = append(git, "history", &facts(), &manifest, &frames, &pool);
    assert!(matches!(result, Err(Error::Wad { ref path, .. }) if path == "DATA/B.wad.client"), "{result:?}");
    assert_eq!(git.rev_parse("history").unwrap(), tip);
}
