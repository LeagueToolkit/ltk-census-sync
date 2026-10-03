//! The manifest list, from a clone of a list repository the test writes.

use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use sync_source::{Error, ListedBuild, ManifestList};

fn git(dir: &Utf8Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([&format!("--git-dir={dir}/.git"), &format!("--work-tree={dir}"), "-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Writes the files, each `(path, manifest id)`, and commits them.
fn commit(dir: &Utf8Path, files: &[(&str, &str)]) -> String {
    for (path, id) in files {
        let path = dir.join(path);
        fs_err::create_dir_all(path.parent().unwrap()).unwrap();
        fs_err::write(&path, format!("https://lol.secure.dyn.riotcdn.net/channels/public/releases/{id}.manifest\n")).unwrap();
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "automatic update"]);
    git(dir, &["rev-parse", "HEAD"])
}

const NA1: &str = "LoL/NA1/windows/lol-game-client";

fn built(version: &str, manifest: u64, commit: &str) -> ListedBuild {
    ListedBuild { version: version.to_string(), manifest, commit: commit.to_string() }
}

#[test]
fn the_builds_added_after_the_checkout_are_the_live_realms_windows_game_client_in_arrival_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let (upstream, clone) = (root.join("upstream"), root.join("clone"));
    fs_err::create_dir_all(&upstream).unwrap();
    git(&upstream, &["init", "-q", "-b", "master"]);
    let first = commit(&upstream, &[(&format!("{NA1}/16.18.8175716.txt"), "344E097FEB027691")]);
    let out = Command::new("git").args(["clone", "-q", upstream.as_str(), clone.as_str()]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let second = commit(
        &upstream,
        &[
            (&format!("{NA1}/16.19.8207193.txt"), "AD6896CE5CF475A1"),
            ("LoL/NA1/macos/lol-game-client/16.19.8207193.txt", "0000000000000001"),
            ("LoL/NA1/windows/lol-standalone-client-content/16.19.8207193.txt", "0000000000000002"),
            ("LoL/EUW1/windows/lol-game-client/16.19.8207193.txt", "AD6896CE5CF475A1"),
        ],
    );
    // One commit with two builds, whose paths sort the other way from their versions.
    let third = commit(&upstream, &[(&format!("{NA1}/16.9.7728292.txt"), "580BD6FA861E4D2C"), (&format!("{NA1}/16.10.7742490.txt"), "404BD6CB5ED42D1F")]);
    // A build the list points at another manifest, and one it drops.
    fs_err::remove_file(upstream.join(format!("{NA1}/16.18.8175716.txt"))).unwrap();
    let fourth = commit(&upstream, &[(&format!("{NA1}/16.19.8207193.txt"), "00000000000000FF")]);

    let list = ManifestList::open(&clone).unwrap();
    assert_eq!(list.checkout().unwrap(), first);
    list.fetch().unwrap();
    assert_eq!(list.checkout().unwrap(), first);
    assert_eq!(list.upstream().unwrap(), fourth);
    let added = list.added(&first, &fourth, "NA1").unwrap();
    assert_eq!(added, [built("16.9.7728292", 0x580B_D6FA_861E_4D2C, &third), built("16.10.7742490", 0x404B_D6CB_5ED4_2D1F, &third), built("16.19.8207193", 0xFF, &fourth)]);
    assert_eq!(list.added(&first, &second, "NA1").unwrap(), [built("16.19.8207193", 0xAD68_96CE_5CF4_75A1, &second)]);
    assert_eq!(list.added(&first, &fourth, "EUW1").unwrap(), [built("16.19.8207193", 0xAD68_96CE_5CF4_75A1, &second)]);
    assert!(list.added(&fourth, &fourth, "NA1").unwrap().is_empty());

    fs_err::write(upstream.join(format!("{NA1}/16.20.1.txt")), "not a URL\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-q", "-m", "automatic update"]);
    list.fetch().unwrap();
    let error = list.added(&first, &list.upstream().unwrap(), "NA1").unwrap_err();
    assert!(matches!(&error, Error::List(message) if message.contains("16.20.1.txt")), "{error}");
}
