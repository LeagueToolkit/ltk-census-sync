//! The checks a commit passes before it is pushed (`docs/APPEND.md`, "Before a push"), against its
//! parent and its build's manifest:
//!
//! - it changes `build.yaml` and files under `files/` only;
//! - every file it adds or changes passes what the tree's `census.yaml` names for its kind;
//! - its message, author and date are the ones its `build.yaml` writes;
//! - its tree is complete: every WAD of the manifest has its `_wad.yaml` with the manifest's file id
//!   and tags, no other WAD has one, and every WAD the commit changed holds an own file for each
//!   entry of its table and for nothing else.

use std::collections::{HashMap, HashSet};

use rayon::prelude::*;
use sync_format::{commit_message, commit_time, wad_dir, wad_tags, Census, AUTHOR};
use sync_source::{read_wad_table, ChunkSource, FileReader, Manifest};

use crate::git::Git;
use crate::tip::{build_facts, is_own_file, wad_ids, Tip};
use crate::Error;

/// The files read together: their texts are held while they are checked.
const BATCH: usize = 20_000;

/// What the check of one commit found.
#[derive(Debug, Default)]
pub struct Checked {
    /// Files added or changed, each checked by its kind.
    pub files: usize,
    /// WADs whose `_wad.yaml` the commit added or changed, each read against its table.
    pub wads: usize,
    /// Each problem, naming the path it is about.
    pub problems: Vec<String>,
}

/// Checks `commit` against its parent and `manifest`, the manifest its `build.yaml` names, whose
/// changed WADs' tables are read from `source` on `pool`.
pub fn check(git: &Git, commit: &str, manifest: &Manifest, source: &dyn ChunkSource, pool: &rayon::ThreadPool) -> Result<Checked, Error> {
    let mut checked = Checked::default();
    let parent = git.rev_parse(&format!("{commit}^"))?;
    let changed = changed(git, &parent, commit)?;

    // What the commit touches.
    let mut texts: Vec<(&str, &str)> = Vec::new();
    for (path, blob) in &changed {
        if path != "build.yaml" && !path.starts_with("files/") {
            checked.problems.push(format!("{path}: an append writes build.yaml and files/ only"));
        } else if let Some(blob) = blob {
            texts.push((path, blob));
        }
    }

    // Every file it adds or changes, by its kind.
    let census_yaml = git.run(&["cat-file", "blob", &format!("{commit}:census.yaml")])?;
    let census = Census::new(&census_yaml, |path| git.run(&["cat-file", "blob", &format!("{commit}:{path}")]).map_err(|e| e.to_string()))
        .map_err(Error::Tip)?;
    for batch in texts.chunks(BATCH) {
        let blobs: Vec<&str> = batch.iter().map(|(_, blob)| *blob).collect();
        let mut read: Vec<Vec<u8>> = Vec::with_capacity(batch.len());
        git.cat(&blobs, |_, bytes| {
            read.push(bytes.to_vec());
            Ok(())
        })?;
        let problems: Vec<String> = pool.install(|| {
            batch
                .par_iter()
                .zip(&read)
                .filter_map(|((path, _), bytes)| {
                    let outcome = std::str::from_utf8(bytes).map_err(|_| "is not UTF-8".to_string()).and_then(|text| census.check(path, text));
                    outcome.err().map(|e| format!("{path}: {e}"))
                })
                .collect()
        });
        checked.files += batch.len();
        checked.problems.extend(problems);
    }

    // The message, author and date.
    let build = git.run(&["cat-file", "blob", &format!("{commit}:build.yaml")])?;
    let facts = build_facts(&build)?;
    if facts.manifest != manifest.id {
        checked.problems.push(format!("build.yaml: manifest {:016x}, checked against {:016x}", facts.manifest, manifest.id));
    }
    let raw = git.run(&["cat-file", "commit", commit])?;
    let (head, message) = raw.split_once("\n\n").unwrap_or((&raw, ""));
    if message != commit_message(&facts) {
        checked.problems.push(format!("the message is not the one build.yaml writes: {message:?}"));
    }
    let time = commit_time(&facts.date).ok_or_else(|| Error::Build(format!("build.yaml: date {:?}", facts.date)))?;
    for role in ["author", "committer"] {
        let expected = format!("{role} {AUTHOR} {time} +0000");
        if !head.lines().any(|line| line == expected) {
            checked.problems.push(format!("the {role} is not {expected:?}"));
        }
    }

    // The tree, against the manifest.
    let listing = git.ls_tree(commit)?;
    let tip = Tip::index(&listing)?;
    let wads: HashMap<String, &sync_source::ManifestFile> =
        manifest.files.iter().filter(|f| f.path.ends_with(".wad.client")).map(|f| (wad_dir(&f.path), f)).collect();
    for dir in tip.wads.keys().filter(|dir| !wads.contains_key(**dir)) {
        checked.problems.push(format!("{dir}: a WAD the manifest does not have"));
    }
    let mut yamls: Vec<(&str, &str)> = Vec::with_capacity(wads.len());
    for dir in wads.keys() {
        match tip.wads.get(dir.as_str()).and_then(|w| w.wad_yaml) {
            Some(blob) => yamls.push((dir, blob)),
            None => checked.problems.push(format!("{dir}: no _wad.yaml")),
        }
    }
    let blobs: Vec<&str> = yamls.iter().map(|(_, blob)| *blob).collect();
    git.cat(&blobs, |i, bytes| {
        let (dir, _) = yamls[i];
        let file = wads[dir];
        let (id, tags) = wad_ids(std::str::from_utf8(bytes).unwrap_or_default());
        if id != Some(file.id) || tags != wad_tags(file.tags.iter().map(String::as_str)) {
            checked.problems.push(format!("{dir}/_wad.yaml: not the manifest's file id {:016x} and tags {:?}", file.id, file.tags));
        }
        Ok(())
    })?;

    // Every WAD the commit changed, against its table.
    let touched: HashSet<&str> = changed.iter().filter_map(|(path, blob)| blob.as_ref().and(path.strip_suffix("/_wad.yaml"))).collect();
    let touched: Vec<&str> = wads.keys().map(String::as_str).filter(|dir| touched.contains(dir)).collect();
    checked.wads = touched.len();
    let problems: Vec<Result<Vec<String>, Error>> = pool.install(|| {
        touched
            .par_iter()
            .map(|dir| {
                let file = wads[*dir];
                let wad_error = |source| Error::Wad { path: file.path.clone(), source };
                let mut reader = FileReader::new(source, manifest.chunks_of(file).map_err(wad_error)?);
                let (_, table) = read_wad_table(&mut reader).map_err(wad_error)?;
                let hashes: HashSet<u64> = table.iter().map(|e| e.path_hash).collect();
                let entries = tip.wads.get(dir).map(|w| &w.entries);
                let mut problems = Vec::new();
                for hash in &hashes {
                    let own = entries.and_then(|e| e.get(hash)).is_some_and(|files| files.iter().any(|(path, _)| is_own_file(path)));
                    if !own {
                        problems.push(format!("{dir}: entry {hash:016x} of the table has no own file"));
                    }
                }
                for hash in entries.into_iter().flat_map(|e| e.keys()).filter(|h| !hashes.contains(h)) {
                    problems.push(format!("{dir}: files for entry {hash:016x}, which the table does not have"));
                }
                Ok(problems)
            })
            .collect()
    });
    for problem in problems {
        checked.problems.extend(problem?);
    }
    Ok(checked)
}

/// The paths a commit changes from its parent, each with its new blob; none for a path it deletes.
fn changed(git: &Git, parent: &str, commit: &str) -> Result<Vec<(String, Option<String>)>, Error> {
    let raw = git.run(&["diff-tree", "-r", "-z", "--no-renames", "--no-commit-id", parent, commit])?;
    let mut fields = raw.split('\0').filter(|f| !f.is_empty());
    let mut changed = Vec::new();
    while let (Some(meta), Some(path)) = (fields.next(), fields.next()) {
        // `:<old mode> <new mode> <old blob> <new blob> <status>`
        let parts: Vec<&str> = meta.split(' ').collect();
        let [_, _, _, blob, status] = parts[..] else { return Err(Error::Tip(format!("a diff-tree record {meta:?}"))) };
        changed.push((path.to_string(), (status != "D").then(|| blob.to_string())));
    }
    Ok(changed)
}
