# Roadmap

What gets built, in order, and what is still open. The history's tip is 16.19.8207193
(2026-09-21); the first real appends are the live builds shipped since.

## Milestones

**0. Workspace and design.** The crates, the docs, the format texts (`census.yaml`, the schemas,
`main`'s README) beside `sync-format`. Done.

**1. `sync-format`: the writers and the facts.** Ported from the code that wrote the history,
without its database: the file names; the YAML families (`build.yaml`, `_wad.yaml`, an entry's own
file with its `links` and its `skeleton`, `mesh`, `texture`, `bank` or `objects`); a bin entry's
ritobin text; and the facts each is written from, read from an entry's bytes -- a bin split into
its links and objects, a bank's media and the wems its events reach, a skeleton's joints, a mesh's
submeshes and position hashes, a texture's format and top mip. Tested against blobs of the
published history, entry by entry. Done ([RUNS.md](RUNS.md)).

**2. `sync-source`: manifests and chunks.** The RMAN manifest reader, keeping each chunk's bundle
layout; the four chunk-hash schemes; files and ranges rebuilt from chunks through `Read + Seek`;
a WAD's table read from its front, and an entry's bytes from its stored ones (a link's as stored,
undecoded). Done ([RUNS.md](RUNS.md)).

**3. `sync-history` and `append`.** The tip's index, the batched blob reader, the fast-import
writer, `census-sync append`. Passes the oracle over 16.10-16.19 appended onto 16.9. Done: 36 of
38 trees and commits identical; the other two are where the published history was written from an
archive's misplaced chunks, and `census-sync rebuild` writes the fix ([RUNS.md](RUNS.md)).

**4. The CDN.** The CDN chunk source with multi-range requests, the chunk cache, recorded fixtures.
Passes the oracle over one patch with the CDN as the only source. Done: the four builds of 16.18
([RUNS.md](RUNS.md)).

**5. Finding builds.** The manifest list, the version, the date, the realms;
`census-sync status`. Done: it lists 16.19.8217343 and 16.19.8230722 after the tip
([RUNS.md](RUNS.md)).

**6. Checks, and runs by hand.** `census-sync check`, and the steps of a run by hand in the README:
status, append, check, `git push`, and the list's checkout moved on (decided 2026-10-03: runs are
by hand for now). Then the builds since 16.19, appended, checked, and pushed to the real history by
the user. Done: `check` passes the published builds of 16.18 and 16.19 ([RUNS.md](RUNS.md)).

**7. On a schedule, when someone wants it.** `census-sync push`, the branch, against a
stand-in remote first; `census-sync run`, a whole run in one command; and a schedule for it, cron in
a jail on a NAS (decided 2026-10-02) or CI, with a deploy key.

**Later.** The manifest code as an `ltk_rman` crate of the LeagueToolkit family, which this
workspace then depends on.

## Open

- **The history's `main`.** Its README names the branch `history` and patch tags until the text
  in `sync-format`'s `main/` is pushed there, which the user does.
- **Banks left unread by that rewrite.** A bank with wems and no events was not read again, so one
  with a `HIRC` section and no events has no `header` or `objects` until its entry changes.
- **The tags of 8.20 to 9.1.** About 200 WADs a build lack locale tags in `_wad.yaml`
  ([RUNS.md](RUNS.md)); the fix changes those commits.
- **The manifest list.** The community mirror is enough to start; asking Riot's patchline
  configuration directly removes the dependency on it.
- **How much the chunk cache keeps.** A build's chunks are almost never read again by the next
  build (0.5% of them over 16.10-16.19, [RUNS.md](RUNS.md)), so the cache serves a run retried or
  re-run more than the next build. Nothing is pruned; it grows by 2 to 4 GB a patch.
- **The pause after large builds.** An append that rewrote several hundred thousand files was
  followed by one to two minutes of nothing before the next build started ([RUNS.md](RUNS.md)).
  Probably the OS writing back the uncompressed pack the import left; measure before changing
  anything.
- **`census.yaml`'s description** names the tool that wrote the first history. It is format text,
  so it stays as it is in format 2.
- **Which parse failures stop a run.** An entry whose bytes do not parse for their kind gets no
  section, as in the history, and the append goes on; [OPERATIONS.md](OPERATIONS.md) has a run stop
  for a file a writer cannot parse in a way it has not seen before. Which failures count as new is
  open.
