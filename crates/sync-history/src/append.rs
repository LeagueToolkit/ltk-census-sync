//! One build to one commit (`docs/APPEND.md`), from the tip and the build's bytes.
//!
//! Only what differs from the tip is written. A WAD whose file id and tags match its `_wad.yaml`
//! carries over and writes nothing; a WAD gone from the manifest is one `D` of its directory; a
//! changed WAD writes its `_wad.yaml`, the files of every entry whose checksum moved, and a `D` for
//! each of the tip's files of it the build no longer has. The changed WADs are read in parallel,
//! and each one's lines go into `git fast-import` as soon as it is read: the order of the lines
//! does not change the tree.
//!
//! The commit is written to a ref of its own, and the branch moves to it only when the import has
//! succeeded and the branch is still at the tip it started from. A run that stops leaves the branch
//! where it was.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::mpsc::sync_channel;

use rayon::prelude::*;
use sync_format::{
    build_yaml, commit_message, commit_time, entry_files, kind_of, wad_dir, wad_tags, wad_yaml, BuildFacts, AUTHOR,
    KIND_LINK,
};
use sync_source::{entry_bytes, read_wad_table, ChunkSource, FileReader, Manifest, ManifestFile, WadEntry};

use crate::git::Git;
use crate::tip::{checksum, is_own_file, wad_ids, Tip, TipWad};
use crate::Error;

/// Where a commit is written before its branch moves to it.
const PENDING: &str = "refs/census-sync/pending";

/// The stored bytes of the entries read together: their chunks come in one request to the source,
/// which the CDN turns into one request a bundle, and are held while the entries are read, on
/// each of the pool's threads.
const BATCH_BYTES: u64 = 32 << 20;

/// What one append did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appended {
    pub commit: String,
    pub tree: String,
    /// The manifest's WADs.
    pub wads: usize,
    /// The WADs whose file id or tags moved, new ones included.
    pub changed: usize,
    /// The entries read, their checksum having moved.
    pub read: usize,
    /// Files written, `build.yaml` and each changed WAD's `_wad.yaml` included.
    pub written: usize,
    /// Files and WAD directories deleted.
    pub removed: usize,
    /// Bin objects that did not parse, so have no `.rito`.
    pub unrenderable: usize,
    /// Bin objects left out under an entry hash their bin repeats.
    pub repeated: usize,
}

/// One changed WAD, read.
struct ReadWad<'a> {
    dir: &'a str,
    wad_yaml: String,
    /// The files of the entries read, by name in the WAD's directory.
    files: Vec<(String, String)>,
    read: HashSet<u64>,
    /// Every path hash of the WAD's table.
    table: HashSet<u64>,
    unrenderable: usize,
    repeated: usize,
}

/// Appends one build to `branch`: its facts, its manifest, and a source of its chunks. The WADs
/// are read on `pool`.
pub fn append(
    git: &Git,
    branch: &str,
    facts: &BuildFacts,
    manifest: &Manifest,
    source: &dyn ChunkSource,
    pool: &rayon::ThreadPool,
) -> Result<Appended, Error> {
    let time = commit_time(&facts.date).ok_or_else(|| Error::Build(format!("{}: date {:?}", facts.version, facts.date)))?;
    let branch_ref = format!("refs/heads/{branch}");
    let tip = git.rev_parse(&branch_ref)?;
    let listing = git.ls_tree(&tip)?;
    let index = Tip::index(&listing)?;

    let wads: Vec<(&ManifestFile, String)> =
        manifest.files.iter().filter(|f| f.path.ends_with(".wad.client")).map(|f| (f, wad_dir(&f.path))).collect();
    let mut dirs = HashSet::new();
    for (file, dir) in &wads {
        if !dirs.insert(dir.as_str()) {
            return Err(Error::Build(format!("{} shares its directory {dir} with another WAD", file.path)));
        }
    }

    // Which WADs carry over: what their `_wad.yaml` says.
    let blobs: Vec<&str> = wads.iter().filter_map(|(_, dir)| index.wads.get(dir.as_str())?.wad_yaml).collect();
    let mut ids = HashMap::with_capacity(blobs.len());
    git.cat(&blobs, |i, text| {
        ids.insert(blobs[i], wad_ids(utf8(text)?));
        Ok(())
    })?;
    let changed: Vec<&(&ManifestFile, String)> = wads
        .iter()
        .filter(|(file, dir)| {
            let was = index.wads.get(dir.as_str()).and_then(|w| w.wad_yaml).and_then(|blob| ids.get(blob));
            !was.is_some_and(|(id, tags)| *id == Some(file.id) && *tags == wad_tags(file.tags.iter().map(String::as_str)))
        })
        .collect();
    tracing::info!("{}: {} of {} WADs changed", facts.version, changed.len(), wads.len());

    // What the changed WADs' entries were: the checksums their own files hold.
    let mut own: Vec<&str> = changed
        .iter()
        .filter_map(|(_, dir)| index.wads.get(dir.as_str()))
        .flat_map(|w| w.entries.values().flatten().filter(|(path, _)| is_own_file(path)).map(|(_, blob)| *blob))
        .collect();
    own.sort_unstable();
    own.dedup();
    let mut checksums: HashMap<&str, u64> = HashMap::with_capacity(own.len());
    git.cat(&own, |i, text| {
        if let Some(c) = checksum(utf8(text)?) {
            checksums.insert(own[i], c);
        }
        Ok(())
    })?;

    let mut import = git.fast_import()?;
    let message = commit_message(facts);
    let mut stats = Appended {
        commit: String::new(),
        tree: String::new(),
        wads: wads.len(),
        changed: changed.len(),
        read: 0,
        written: 1,
        removed: 0,
        unrenderable: 0,
        repeated: 0,
    };
    {
        let out = import.stream();
        write!(out, "commit {PENDING}\nauthor {AUTHOR} {time} +0000\ncommitter {AUTHOR} {time} +0000\n")?;
        write!(out, "data {}\n{message}from {tip}\n", message.len())?;
        inline(out, "build.yaml", &build_yaml(facts))?;
        for dir in index.wads.keys().filter(|dir| !dirs.contains(*dir)) {
            writeln!(out, "D {dir}")?;
            stats.removed += 1;
        }
    }

    let (tx, rx) = sync_channel::<Result<ReadWad, Error>>(2 * pool.current_num_threads());
    std::thread::scope(|scope| -> Result<(), Error> {
        let (index, checksums) = (&index, &checksums);
        scope.spawn(move || {
            pool.install(|| {
                changed.par_iter().try_for_each_with(tx, |tx, (file, dir)| {
                    let read = read_wad(file, dir, index.wads.get(dir.as_str()), checksums, manifest, source, facts.legacy_bins);
                    tx.send(read).map_err(drop)
                })
            })
        });
        for read in rx {
            let read = read?;
            stats.read += read.read.len();
            stats.unrenderable += read.unrenderable;
            stats.repeated += read.repeated;
            let out = import.stream();
            inline(out, &format!("{}/_wad.yaml", read.dir), &read.wad_yaml)?;
            stats.written += 1 + read.files.len();
            let names: HashSet<&str> = read.files.iter().map(|(name, _)| name.as_str()).collect();
            for (name, text) in &read.files {
                inline(out, &format!("{}/{name}", read.dir), text)?;
            }
            let Some(old) = index.wads.get(read.dir) else { continue };
            for (hash, files) in &old.entries {
                let gone = !read.table.contains(hash);
                if !gone && !read.read.contains(hash) {
                    continue;
                }
                for (path, _) in files {
                    if gone || !names.contains(&path[read.dir.len() + 1..]) {
                        writeln!(out, "D {path}")?;
                        stats.removed += 1;
                    }
                }
            }
        }
        Ok(())
    })?;

    import.finish()?;
    let commit = git.rev_parse(PENDING)?;
    let tree = git.rev_parse(&format!("{commit}^{{tree}}"))?;
    git.update_ref(&branch_ref, &commit, Some(&tip))?;
    git.delete_ref(PENDING)?;
    Ok(Appended { commit, tree, ..stats })
}

/// An `M` line with its text inline.
fn inline(out: &mut impl Write, path: &str, text: &str) -> std::io::Result<()> {
    write!(out, "M 100644 inline {path}\ndata {}\n", text.len())?;
    out.write_all(text.as_bytes())?;
    out.write_all(b"\n")
}

fn utf8(bytes: &[u8]) -> Result<&str, Error> {
    std::str::from_utf8(bytes).map_err(|_| Error::Tip("a blob that is not UTF-8".into()))
}

/// A changed WAD's table, and the files of every entry whose checksum is not its tip's.
fn read_wad<'a>(
    file: &ManifestFile,
    dir: &'a str,
    old: Option<&TipWad>,
    checksums: &HashMap<&str, u64>,
    manifest: &Manifest,
    source: &dyn ChunkSource,
    legacy: bool,
) -> Result<ReadWad<'a>, Error> {
    let wad_error = |source: sync_source::Error| Error::Wad { path: file.path.clone(), source };
    let mut reader = FileReader::new(source, manifest.chunks_of(file).map_err(wad_error)?);
    let (header, table) = read_wad_table(&mut reader).map_err(wad_error)?;
    // A path hash the table lists twice takes its first entry's bytes and its last entry's
    // checksum, as the export that wrote the history stored it.
    let mut first: Vec<WadEntry> = Vec::new();
    let mut last: HashMap<u64, u64> = HashMap::with_capacity(table.len());
    for entry in &table {
        if last.insert(entry.path_hash, entry.checksum).is_none() {
            first.push(*entry);
        }
    }
    let was = |hash: u64| -> Option<u64> {
        let files = old?.entries.get(&hash)?;
        files.iter().find(|(path, _)| is_own_file(path)).and_then(|(_, blob)| checksums.get(blob)).copied()
    };
    let mut read = ReadWad {
        dir,
        wad_yaml: wad_yaml(header.major, header.minor, file.id, file.tags.iter().map(String::as_str)),
        files: Vec::new(),
        read: HashSet::new(),
        table: last.keys().copied().collect(),
        unrenderable: 0,
        repeated: 0,
    };
    let moved: Vec<WadEntry> = first.into_iter().filter(|e| was(e.path_hash) != Some(last[&e.path_hash])).collect();
    for batch in batches(&moved) {
        let ranges: Vec<(u64, u64)> = batch.iter().map(|e| (e.offset, e.stored_size)).collect();
        reader.preload(&ranges).map_err(wad_error)?;
        for entry in batch {
            let checksum = last[&entry.path_hash];
            let stored = reader.read_range(entry.offset, entry.stored_size).map_err(wad_error)?;
            let bytes = entry_bytes(&stored, entry).map_err(wad_error)?;
            let kind = if entry.is_link() { KIND_LINK } else { kind_of(&bytes) };
            let files = entry_files(entry.path_hash, Some(checksum), kind, &bytes, legacy);
            if let Some(error) = &files.error {
                tracing::debug!("{} {:016x}: {error}", file.path, entry.path_hash);
            }
            read.read.insert(entry.path_hash);
            read.unrenderable += files.unrenderable;
            read.repeated += files.repeated;
            read.files.extend(files.files);
        }
    }
    Ok(read)
}

/// `entries` cut into runs of about `BATCH_BYTES` stored bytes, in order; an entry larger than
/// that is a run of its own.
fn batches(entries: &[WadEntry]) -> Vec<&[WadEntry]> {
    let mut out = Vec::new();
    let (mut start, mut bytes) = (0, 0);
    for (i, entry) in entries.iter().enumerate() {
        if i > start && bytes + entry.stored_size > BATCH_BYTES {
            out.push(&entries[start..i]);
            (start, bytes) = (i, 0);
        }
        bytes += entry.stored_size;
    }
    if start < entries.len() {
        out.push(&entries[start..]);
    }
    out
}
