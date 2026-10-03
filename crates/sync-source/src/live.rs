//! The live builds (`docs/SOURCES.md`, "Live builds"): the community manifest list, a git
//! repository with one file per build holding its manifest's URL, read from a local clone. The
//! clone's checkout is the last list commit whose builds were appended. A fetch brings the commits
//! after it, and the builds they add or change for the live realm are the candidates.

use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};

use crate::Error;

/// The realm the history follows as live (decided 2026-10-03): a build is appended when this realm
/// ships it, and its `realms` is this one.
pub const LIVE_REALM: &str = "NA1";

/// One build the list records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedBuild {
    /// The client's version, from the file's name: `16.19.8207193`.
    pub version: String,
    pub manifest: u64,
    /// The list commit that added the file or last changed it.
    pub commit: String,
}

/// A local clone of the manifest list.
pub struct ManifestList {
    git_dir: Utf8PathBuf,
}

impl ManifestList {
    /// The clone at `root`: its working tree, or its git directory when it has none.
    pub fn open(root: &Utf8Path) -> Result<Self, Error> {
        let dot_git = root.join(".git");
        let list = Self { git_dir: if dot_git.is_dir() { dot_git } else { root.to_path_buf() } };
        list.run(&["rev-parse", "--git-dir"])?;
        Ok(list)
    }

    /// Fetches the list's remote. The checkout does not move.
    pub fn fetch(&self) -> Result<(), Error> {
        self.run(&["fetch", "--quiet", "origin"]).map(drop)
    }

    /// The commit checked out: the last one whose builds were appended.
    pub fn checkout(&self) -> Result<String, Error> {
        self.rev_parse("HEAD")
    }

    /// The remote's commit as of the last fetch.
    pub fn upstream(&self) -> Result<String, Error> {
        self.rev_parse("refs/remotes/origin/HEAD")
    }

    /// The Windows game-client builds of `realm` that the commits after `from` up to `to` add or
    /// change, in the order they arrived. A build changed twice is listed once, at its last change.
    pub fn added(&self, from: &str, to: &str, realm: &str) -> Result<Vec<ListedBuild>, Error> {
        let dir = format!("LoL/{realm}/windows/lol-game-client/");
        let range = format!("{from}..{to}");
        let log = self.run(&["log", "--reverse", "--no-renames", "--diff-filter=AM", "--format=commit %H", "--name-only", &range, "--", &dir])?;
        let mut builds: Vec<(usize, ListedBuild)> = Vec::new();
        let (mut commit, mut index) = ("", 0);
        for line in log.lines().filter(|l| !l.is_empty()) {
            if let Some(id) = line.strip_prefix("commit ") {
                (commit, index) = (id, index + 1);
                continue;
            }
            let Some(version) = line.strip_prefix(dir.as_str()).and_then(|name| name.strip_suffix(".txt")) else { continue };
            let url = self.run(&["show", &format!("{commit}:{line}")])?;
            let manifest = manifest_of(url.trim()).ok_or_else(|| Error::List(format!("{line} at {commit}: {:?} is not a manifest URL", url.trim())))?;
            builds.retain(|(_, b)| b.version != version);
            builds.push((index, ListedBuild { version: version.to_string(), manifest, commit: commit.to_string() }));
        }
        // One list commit can add several builds, named in path order; the build number orders them.
        builds.sort_by_key(|(index, b)| (*index, version_key(&b.version)));
        Ok(builds.into_iter().map(|(_, b)| b).collect())
    }

    fn rev_parse(&self, rev: &str) -> Result<String, Error> {
        Ok(self.run(&["rev-parse", "--verify", "--end-of-options", rev])?.trim().to_string())
    }

    fn run(&self, args: &[&str]) -> Result<String, Error> {
        let out = Command::new("git").arg(format!("--git-dir={}", self.git_dir)).args(args).output()?;
        if !out.status.success() {
            return Err(Error::List(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim())));
        }
        String::from_utf8(out.stdout).map_err(|_| Error::List(format!("git {}: printed bytes that are not UTF-8", args.join(" "))))
    }
}

/// A version's numbers, `16.19.8207193` as `[16, 19, 8207193]`, so `16.9` sorts before `16.10`.
fn version_key(version: &str) -> Vec<u64> {
    version.split('.').map(|part| part.parse().unwrap_or(u64::MAX)).collect()
}

/// The id a manifest URL names: `.../releases/580BD6FA861E4D2C.manifest`.
fn manifest_of(url: &str) -> Option<u64> {
    let name = url.rsplit('/').next()?.strip_suffix(".manifest")?;
    (name.len() == 16).then(|| u64::from_str_radix(name, 16).ok()).flatten()
}
