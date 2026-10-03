# ltk-census-sync

The maintainer tool for the League of Legends census history: for each new live build of the game
client, it appends one commit to the history repository, computed from that repository's tip and
the new build's own bytes. Reading the history needs only git; this tool is for keeping it up to
date.

## The history

[ltk-census-history](https://github.com/moonshadow565/ltk-census-history) holds every live build
of the Windows game client from patch 8.20 (October 2018), **one commit per build and one file
per fact**: a WAD, a WAD entry, a bin entry. A patch reads as a diff. It is used to track how the
game's data changes from patch to patch and to help port mods across patches, and it may serve
data mining.

What it holds and in what exact form is format 2, frozen: [docs/FORMAT.md](docs/FORMAT.md). Every
tree also carries `census.yaml` and a JSON Schema per kind of YAML file, so a checkout says how to
read itself.

The builds up to 16.19 were written by a separate, local export from a database of past builds
(8.20 to 9.1 read from Riot's older RADS distribution, the rest from an archive of RMAN
manifests and bundles). From 16.19 on, the history is kept up by appending, and the history itself
is the only state: no database.

## What a run does

1. **Find** the live builds the history does not have yet.
2. **Plan** each one against the tip: a WAD whose file id and tags are unchanged carries over
   whole; in a changed WAD, an entry whose checksum is unchanged keeps its files.
3. **Fetch** only what is new: the changed WADs' tables of contents, then the bytes of the changed
   entries, as chunks from a local cache or Riot's CDN by byte range.
4. **Render** each new entry's files from its bytes, with the same writers that produced the
   history.
5. **Commit** on top of the tip, check the result, and **push**.

The details: [docs/APPEND.md](docs/APPEND.md) for the commit, [docs/SOURCES.md](docs/SOURCES.md)
for builds and bytes, [docs/OPERATIONS.md](docs/OPERATIONS.md) for running it on a schedule.

## Status

Being written ([docs/ROADMAP.md](docs/ROADMAP.md)). The writers (`sync-format`) reproduce the
published history, the manifest and chunk reader (`sync-source`) reads past builds from an
archive, and `census-sync append` reproduces the published builds of 16.10 to 16.19; the CDN as a
chunk source is next.

## Layout

```
crates/sync-format    format 2: file names, YAML writers, bin entries as ritobin text, facts from bytes
crates/sync-source    live builds, RMAN manifests, chunks from a cache or the CDN, files from chunks
crates/sync-history   the history repository: the tip, the next commit, tags, notes, checks
crates/sync-cli       the census-sync binary
docs/                 the design, one file per area
```
