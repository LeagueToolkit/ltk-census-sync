//! The builds a branch holds, by the manifest id in each commit's `Census-Manifest` trailer
//! (`docs/FORMAT.md`).

use std::collections::HashSet;

use crate::git::Git;
use crate::Error;

/// Every manifest id the commits up to `rev` hold.
pub fn manifests(git: &Git, rev: &str) -> Result<HashSet<u64>, Error> {
    let log = git.run(&["log", "--format=%(trailers:key=Census-Manifest,valueonly,separator=)", rev])?;
    log.lines()
        .filter(|line| !line.is_empty())
        .map(|line| u64::from_str_radix(line.trim(), 16).map_err(|_| Error::Tip(format!("a Census-Manifest trailer {line:?}"))))
        .collect()
}
