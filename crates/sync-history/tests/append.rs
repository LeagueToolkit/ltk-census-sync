//! An append onto a small history the test writes, in a scratch repository.

mod common;

use std::collections::BTreeMap;

use common::{facts, history, manifest, own, wad};
use sync_format::{build_yaml, commit_message, entry_files, wad_yaml};
use sync_history::{append, Error, Git};

const KEPT: u64 = 0x1111_1111_1111_1111;
const REREAD: u64 = 0x2222_2222_2222_2222;
const GONE: u64 = 0x3333_3333_3333_3333;
const NEW: u64 = 0x4444_4444_4444_4444;

fn checksum_yaml(checksum: u64) -> String {
    format!("sha256: \"{}\"\nchecksum: \"{checksum:016x}\"\n", "00".repeat(32))
}

fn tree(git: &Git, rev: &str) -> BTreeMap<String, String> {
    git.run(&["ls-tree", "-r", "--format=%(path) %(objectname)", rev])
        .unwrap()
        .lines()
        .map(|l| l.split_once(' ').map(|(p, b)| (p.to_string(), b.to_string())).unwrap())
        .collect()
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

