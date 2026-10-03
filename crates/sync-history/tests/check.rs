//! The checks before a push, on an append and on a commit made wrong on purpose.

mod common;

use std::io::Write;

use common::{facts, history, manifest, own, wad};
use sync_format::{wad_yaml, CENSUS_YAML, SCHEMAS};
use sync_history::{append, check};

const KEPT: u64 = 0x1111_1111_1111_1111;
const NEW: u64 = 0x4444_4444_4444_4444;
const STRAY: u64 = 0x5555_5555_5555_5555;

#[test]
fn an_append_passes_and_a_commit_made_wrong_fails_each_check() {
    let mut base = vec![("census.yaml".to_string(), CENSUS_YAML.to_string()), ("build.yaml".to_string(), "version: \"16.19.1\"\n".to_string())];
    base.extend(SCHEMAS.iter().map(|(path, text)| (path.to_string(), text.to_string())));
    let scratch = history(&base);
    let git = &scratch.git;
    let (manifest, frames) = manifest(&[("DATA/B.wad.client", 0xB1, wad(&[(KEPT, 5, b"kept"), (NEW, 6, b"new")]), true)]);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
    let appended = append(git, "history", &facts(), &manifest, &frames, &pool).unwrap();

    let checked = check(git, &appended.commit, &manifest, &frames, &pool).unwrap();
    assert_eq!(checked.problems, Vec::<String>::new());
    assert_eq!((checked.files, checked.wads), (4, 1));

    // On top of it: another census.yaml, an entry file with a field the schema lacks, a bin entry
    // that is not ritobin, a `_wad.yaml` with another file id, an entry of the table without its
    // file, files for an entry the table lacks, and another message and date.
    let dir = "files/data/b.wad.client";
    let mut import = git.fast_import().unwrap();
    let out = import.stream();
    let message = "16.20.1\n";
    write!(out, "commit refs/heads/history\ncommitter census <census@localhost> 1 +0000\ndata {}\n{message}from {}\n", message.len(), appended.commit).unwrap();
    for (path, text) in [
        ("census.yaml".to_string(), format!("{CENSUS_YAML}# more\n")),
        (own(dir, NEW), format!("sha256: \"{}\"\nextra: \"1\"\n", "00".repeat(32))),
        (format!("{dir}/44/{NEW:016x}.bin/aa/aaaaaaaa.rito"), "not ritobin {\n".to_string()),
        (format!("{dir}/_wad.yaml"), wad_yaml(3, 4, 0xB2, [])),
        (own(dir, STRAY), format!("sha256: \"{}\"\n", "00".repeat(32))),
    ] {
        write!(out, "M 100644 inline {path}\ndata {}\n{text}\n", text.len()).unwrap();
    }
    writeln!(out, "D {}", own(dir, KEPT)).unwrap();
    import.finish().unwrap();

    let wrong = git.rev_parse("history").unwrap();
    let problems = check(git, &wrong, &manifest, &frames, &pool).unwrap().problems;
    let expected = [
        "census.yaml: an append writes build.yaml and files/ only".to_string(),
        format!("{}: files/**/??/????????????????.yaml: /extra is not a field the schema has", own(dir, NEW)),
        format!("{dir}/44/{NEW:016x}.bin/aa/aaaaaaaa.rito: does not parse as ritobin"),
        format!("{dir}/_wad.yaml: not the manifest's file id 00000000000000b1"),
        format!("{dir}: entry {KEPT:016x} of the table has no own file"),
        format!("{dir}: files for entry {STRAY:016x}, which the table does not have"),
        format!("the message is not the one build.yaml writes: {message:?}"),
        format!("the author is not {:?}", "author census <census@localhost> 1790812800 +0000"),
        format!("the committer is not {:?}", "committer census <census@localhost> 1790812800 +0000"),
    ];
    for want in &expected {
        assert!(problems.iter().any(|p| p.starts_with(want.as_str())), "no problem starts {want:?} in {problems:#?}");
    }
    assert_eq!(problems.len(), expected.len(), "{problems:#?}");
}
