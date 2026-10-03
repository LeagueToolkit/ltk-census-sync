# Append

How one new build becomes one commit on `history`, from the tip and the build's own bytes. The
history is the only state: what the tip holds says what each WAD and entry was, and nothing else is
kept between runs. The format of what is written is [FORMAT.md](FORMAT.md).

## Inputs

- **The tip** of `history`, in a local clone ([OPERATIONS.md](OPERATIONS.md)).
- **The build**: its manifest id, version, date and realms ([SOURCES.md](SOURCES.md)). The
  property-kind numbering of its bins follows from its patch: legacy before 10.8, so never for a
  build appended from here on.
- **Its bytes**, as chunks by id from the cache or the CDN.

## What the tip says

`git ls-tree -r` lists the tip; each path under `files/` belongs to one WAD directory and, below
`_wad.yaml`, to one entry:

- `<dir>/_wad.yaml`: the WAD.
- `<dir>/xx/<hash>.yaml`: entry `<hash>`'s own file.
- `<dir>/xx/<hash>.bin/yy/<entry>.rito`: one of entry `<hash>`'s bin entries.

Blobs are read with one `git cat-file --batch` fed from a thread, so requests and replies stream:
the `_wad.yaml` of every WAD the manifest names, and the own file of every entry of a WAD that
changed.

## The plan, WAD by WAD

Only the manifest's `.wad.client` files are read; the history holds nothing about other files.

1. **A WAD whose `fileId` and tags match its `_wad.yaml` carries over whole**: every path under
   its directory keeps its blob. Nothing of it is fetched.
2. **Any other WAD is changed.** Its table of contents is read (a range at the front of the file),
   and its `_wad.yaml` is written from the header's version and the manifest's file id and tags.
3. **In a changed WAD, an entry whose checksum equals the one in its own file keeps its files**,
   its bin's entries included. A path hash that appears twice in one table takes its first entry's
   content and its last entry's checksum.
4. **Every other entry is read**: its bytes decompressed and hashed, its kind taken from its magic
   (or `link`, for an entry the table stores as a link to another), and its files rendered by
   `sync-format`.
5. **Whatever the tip held that the new tree lacks is deleted**: a WAD gone from the manifest, an
   entry gone from its table, a bin entry gone from its bin.

`build.yaml` is written new; `.gitattributes`, `census.yaml` and `schema/` carry over.

Every file is a function of one WAD's or one entry's bytes, so nothing is reconciled across WADs,
and the WADs are read in parallel.

## The commit

Streamed into `git fast-import` on top of the tip: `commit refs/heads/history`, the author,
committer and message of [FORMAT.md](FORMAT.md), `from <tip>`, one `M` line per new or changed
path with its text inline, one `D` line per deleted path. A path that keeps the tip's blob is not
written. The stream is written through a buffer as it is produced: a build that
rewrites many bins is gigabytes of text, more than one pipe write takes or memory should hold.

fast-import deduplicates blobs against what the repository holds, so an unchanged bin entry
rewritten under a changed bin costs nothing.

## Before a push

The commit is checked, and pushed only if every check passes; otherwise the branch is reset to the
old tip and the run stops ([OPERATIONS.md](OPERATIONS.md)):

- every new or changed YAML file validates against the schema `census.yaml` names for it;
- every `.rito` parses back with ltk_ritobin;
- the tree is complete: every `.wad.client` of the manifest has its `_wad.yaml`, and every entry of
  every changed WAD has its own file.

## Decisions an append forces

- **Commit order is arrival order** (decided 2026-10-02). The builds up to 16.19 are in patch order;
  an append lands builds as they ship, so a hotfix for one patch released after the next patch
  started comes after it. That is what happened; a full rebuild that wants to reproduce it sorts by
  date.
- **A realm that adopts a build later** is recorded in a git note, `refs/notes/realms`, on the
  build's commit (decided 2026-10-02). `build.yaml` holds the realms known at append time, and an
  old commit is never rewritten.
- **Tags in arrival order** are open ([ROADMAP.md](ROADMAP.md)): a patch's last build is known only
  once the next patch ships.
- **No PBE.** Live realms only.

## The oracle

An append and the history it extends agree byte for byte, so the history is its own test: cut
`history` at some build, append the builds that follow it from their bytes, and compare each tree
id with the published commit's. It needs no database and no second implementation
([TOOLING.md](TOOLING.md)).
