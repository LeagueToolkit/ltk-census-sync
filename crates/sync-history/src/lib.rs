//! The history repository (`docs/APPEND.md`): what its tip holds, read with `git ls-tree` and
//! `git cat-file --batch`; the next commit, streamed into `git fast-import` on top of it; tags,
//! notes, and the checks a commit passes before it is pushed.
//!
//! git is driven as a process. Its own object writing and packing are what the history's size was
//! measured with, and nothing here reimplements them.
