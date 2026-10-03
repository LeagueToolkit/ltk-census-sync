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
published history, entry by entry.

**2. `sync-source`: manifests and chunks.** The RMAN manifest reader, keeping each chunk's bundle
layout; the four chunk-hash schemes; files and ranges rebuilt from chunks through `Read + Seek`;
a WAD's table read from its front. A local merged bundle as the first chunk source, so the
oracle can run before the CDN source exists.

**3. `sync-history` and `append`.** The tip's index, the batched blob reader, the fast-import
writer, `census-sync append`. Passes the oracle over 16.10-16.19 appended onto 16.9: 38 trees, all
identical.

**4. The CDN.** The CDN chunk source with multi-range requests, the chunk cache, recorded fixtures.
Passes the oracle over one patch with the CDN as the only source.

**5. Finding builds.** The manifest list, the version, the date, the realms;
`census-sync status`.

**6. Checks and pushing.** `census-sync check` and `push`, against a stand-in remote first. Then the
builds since 16.19, appended, checked, and pushed to the real history by the user.

**7. On a schedule.** `census-sync run` from cron in a jail on a NAS, with a deploy key.

**Later.** The manifest code as an `ltk_rman` crate of the LeagueToolkit family, which this
workspace then depends on; a GitHub-hosted schedule as an alternative to the NAS.

## Open

- **Tags in arrival order.** A tag marks a patch's last build, which is known only once the next
  patch ships, and a hotfix can still arrive after that. Either tag a patch when the first build
  of the next one is appended and never move it (a later hotfix lands after the tag), or move the
  tag to each new build of its patch (a reader has to fetch tags with `--force`). The first keeps
  tags immutable and is the one proposed.
- **Realm notes.** What a note in `refs/notes/realms` holds when a realm adopts a build after it was
  appended: proposed, the full sorted realm list as of the note, one line, so the newest note is
  the answer.
- **The HTTP client** for manifests and multi-range bundle requests. `ureq` is blocking with no
  async runtime, which suits a tool whose parallelism is rayon's; `reqwest` with `blocking` is what
  ltk-manager uses, at the cost of a tokio runtime inside. Either needs a hand-written
  `multipart/byteranges` reader. Proposed: `ureq`.
- **The manifest list.** The community mirror is enough to start; asking Riot's patchline
  configuration directly removes the dependency on it.
- **The chunk cache.** How much to keep, and whether to keep it at all once a build is pushed: the
  next build shares most chunks with the last.
- **The pause after large builds.** An append that rewrote several hundred thousand files was
  followed by one to two minutes of nothing before the next build started ([RUNS.md](RUNS.md)).
  Probably the OS writing back the uncompressed pack the import left; measure before changing
  anything.
- **`census.yaml`'s description** names the tool that wrote the first history. It is format text,
  so it stays as it is in format 2.
- **`main`'s README** says the history is kept up by `census append`. `main` is in no `history`
  tree, so a new README is a child commit on `main`, with no effect on the oracle; it goes with
  census-sync's first push.
- **License** of this repository.
