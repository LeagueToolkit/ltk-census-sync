//! Patch tags (`docs/FORMAT.md`): a lightweight tag named for a patch marks its newest build when
//! the next patch's first build is appended, and never moves (decided 2026-10-03). A build of a
//! patch that arrives after the next one began lands after its patch's tag.

use sync_format::BuildFacts;

use crate::git::Git;
use crate::tip::build_facts;
use crate::Error;

/// Tags `tip` with its patch when `next`, the build appended onto it, is of a later patch and the
/// tip's patch has no tag yet. The patch tagged, if one was.
pub fn tag_patch(git: &Git, tip: &str, next: &BuildFacts) -> Result<Option<String>, Error> {
    let facts = build_facts(&git.run(&["cat-file", "blob", &format!("{tip}:build.yaml")])?)?;
    if patch_key(&next.patch) <= patch_key(&facts.patch) {
        return Ok(None);
    }
    let name = format!("refs/tags/{}", facts.patch);
    if !git.run(&["for-each-ref", "--format=%(refname)", &name])?.trim().is_empty() {
        return Ok(None);
    }
    git.update_ref(&name, tip, None)?;
    Ok(Some(facts.patch))
}

/// A patch's numbers, `16.19` as `[16, 19]`, so `16.9` sorts before `16.10`.
fn patch_key(patch: &str) -> Vec<u32> {
    patch.split('.').map(|part| part.parse().unwrap_or(u32::MAX)).collect()
}
