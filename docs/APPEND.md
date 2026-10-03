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

Streamed into `git fast-import`: the author, committer and message of [FORMAT.md](FORMAT.md),
`from <tip>`, then only what differs from the tip:

- `build.yaml`;
- a `D` line for the directory of each WAD gone from the manifest;
- for each changed WAD, its `_wad.yaml`, an `M` line with its text inline for every file of every
  entry read, and a `D` line for each of the tip's files of the WAD that the build no longer has:
  an entry gone from its table, a bin entry gone from its bin.

A WAD that carries over and an entry that keeps its files write nothing. The changed WADs are read
in parallel, and each one's lines go into the stream as soon as it is read; their order does not
change the tree. The stream is written through a buffer as it is produced: a build that rewrites
many bins is gigabytes of text, more than one pipe write takes or memory should hold.

The commit is written to `refs/census-sync/pending`, and `history` moves to it only when the import
has succeeded, and only from the tip the append started from. An append that stops leaves
`history` where it was.

fast-import deduplicates blobs against what the repository holds, so an unchanged bin entry
rewritten under a changed bin costs nothing.

## Before a push

Each commit not yet pushed is checked against its parent and its build's manifest, and pushed
only if every check passes; otherwise the run stops and nothing is pushed
([OPERATIONS.md](OPERATIONS.md)):

- it changes `build.yaml` and files under `files/` only;
- every YAML file it adds or changes validates against the schema the tree's `census.yaml` names
  for it;
- every `.rito` it adds or changes parses with ltk_ritobin, with no error and no diagnostic, into a
  bin that prints as the same text;
- its message, author and date are the ones its `build.yaml` writes;
- the tree is complete: every `.wad.client` of the manifest has its `_wad.yaml`, holding the
  manifest's file id and tags, no other WAD has one, and every WAD the commit changed holds an own
  file for each entry of its table, read again from the build's bytes, and for no other entry.

## Decisions an append forces

- **Commit order is arrival order** (decided 2026-10-02). The builds up to 16.19 are in patch order;
  an append lands builds as they ship, so a hotfix for one patch released after the next patch
  started comes after it. That is what happened; a full rebuild that wants to reproduce it sorts by
  date.
- **One realm.** A build's `realms` is `NA1`, the realm census-sync follows (decided
  2026-10-03), so no realm adopts a build later and an old commit is never revisited for one.
- **A tag is set once** (decided 2026-10-03). Appending the first build of a later patch tags the
  tip with its patch, unless that patch has a tag already; a tag never moves, and a build that
  arrives after its patch's tag lands after it. A patch's last build is known only once the next
  patch ships, and moving a tag would make every reader fetch tags with `--force`.
- **No PBE.** The live realm only.

## The oracle

An append and the history it extends agree byte for byte, so the history is its own test: cut
`history` at some build, append the builds that follow it from their bytes, and compare each tree
id with the published commit's. It needs no database and no second implementation
([TOOLING.md](TOOLING.md)).
