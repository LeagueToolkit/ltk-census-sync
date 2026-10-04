//! The history repository (`docs/APPEND.md`): what its tip holds, read with `git ls-tree` and
//! `git cat-file --batch`; the builds its commits hold; the next commit, streamed into
//! `git fast-import` on top of it; and the checks a commit passes before it is pushed.
//!
//! git is driven as a process. Its own object writing and packing are what the history's size was
//! measured with, and nothing here reimplements them.

mod append;
mod builds;
mod check;
mod error;
mod git;
mod tip;

pub use append::{append, Appended};
pub use builds::manifests;
pub use check::{check, Checked};
pub use error::Error;
pub use git::{FastImport, Git};
pub use tip::{build_facts, checksum, is_own_file, wad_ids, Tip, TipWad};
