# Sources

Where a run learns which builds to append and where their bytes come from.

## Live builds

The history tracks the Windows game client (`lol-game-client`) on the live realms, never PBE. A
build is new when its manifest id is in no commit's `Census-Manifest` trailer.

**The manifest list.** The community mirror `github.com/Morilli/riot-manifests` records every
manifest Riot's patchlines publish, one file per build: `LoL/<realm>/windows/lol-game-client/<version>.txt`,
holding the manifest's URL. A `git fetch` of the mirror, diffed against the last run, lists the new
ones. The file name gives the version (`16.19.8207193`) and the directory the realm; one manifest
ships on several realms, which together are the build's `realms`.

**The version** can also be read from the build itself: `League of Legends.exe`'s version resource
(`VS_FIXEDFILEINFO`; the build number is the third and fourth parts, the fourth zero-padded to four
digits). It is one file of about 30 MB, so a run that cannot trust a mirror's file name pays that
once a build.

**The date** is the day the manifest was published: its `Last-Modified` from Riot's CDN, as UTC.
Recorded when the manifest is first fetched, since nothing in the manifest dates it.

## RMAN manifests

A manifest lists a build's files -- path, size, file id, language tags -- and each file's chunks.
Its body is zstd-compressed flatbuffers: files, chunks, bundles, languages, chunking parameters.

```
https://lol.secure.dyn.riotcdn.net/channels/public/releases/{MANIFEST_ID:016X}.manifest
```

A file is its chunks in order. A chunk is identified by the first eight bytes of a hash of its
**uncompressed** bytes, the hash chosen per file by its chunking parameters' version:

| version | hash | the game client's manifests |
| --- | --- | --- |
| 1 | SHA-512 | not seen |
| 2 | SHA-256 | not seen |
| 3 | RITO_HKDF: PBKDF2-HMAC-SHA256 over the SHA-256, empty salt, 32 rounds | 9.2 to 16.3 |
| 4 | BLAKE3 | from 16.4.7461248 |

Every chunk is checked after decompression: its hash under its file's scheme and the uncompressed
size the manifest gives. A chunk is those three: its id, the hash its id is under, and its
uncompressed size. The same chunk can be compressed differently in different bundles, so
compressed bytes and sizes are never compared or reused across bundles.

## Bundles and the CDN

Chunks live in bundles: a bundle is a sequence of chunks, each zstd-compressed on its own, a few
tens of megabytes in all.

```
https://lol.dyn.riotcdn.net/channels/public/bundles/{BUNDLE_ID:016X}.bundle
```

The manifest host and the bundle host differ. The manifest gives, per bundle, its chunks in order
with their compressed and uncompressed sizes; a chunk's offset in its bundle is the running sum of
the compressed sizes before it. A downloader keeps that layout for the run; it describes the
bundles of this manifest and no other.

**Ranges, not bundles.** Changed entries are scattered, so the chunks a run needs are a few hundred
kilobytes here and there across many bundles, about a tenth of the bundles' bytes
([RUNS.md](RUNS.md)). The wanted chunks of one bundle go in one request as a multi-range `Range`
header, chunks that lie end to end in one span, at most 128 spans a request. The CDN answers `206`
with a `multipart/byteranges` body, or for one span a `206` with a `Content-Range` header. Past
about 258 spans it has answered with the whole bundle, a `200`, which is not an error; one edge
answered 1,000 spans with parts ([RUNS.md](RUNS.md)). An answer is read by its shape, and each part
by the length its `Content-Range` gives. An answer has left out a part it was asked for
([RUNS.md](RUNS.md)), so the chunks an answer lacks are asked for again, three times at most before
the run stops.

## Chunk sources

A chunk is asked for **by its id, its hash and its uncompressed size**, and each source finds it
its own way and checks all three. The sources are asked in order, and a chunk comes from the first
that holds it with bytes that check:

- **A mirror**, when one is given: Riot's bundles kept whole at their CDN paths,
  `channels/public/bundles/<BUNDLE ID>.bundle` under one directory, read by the layout of the
  manifest being read, as from the CDN. The directory can be served over HTTP as a mirror of the
  CDN. A run reads it and never writes it.
- **The CDN**, over https, read through **the chunk cache**: one fjall database on local disk that
  keeps each frame the CDN gave under the three things its chunk is: the chunking parameter version
  of its hash, its uncompressed size and its id (chosen 2026-10-03). A chunk the cache holds with a
  frame that checks comes from the cache; any other is fetched by range, checked, and kept. So a
  chunk is downloaded once, and a frame is never handed out for a chunk of another scheme or size
  that shares its id. A frame that does not check is fetched again and replaced. A run offline
  reads the cache alone.

No source is trusted more than the CDN. An archive of past builds held chunks' bytes under other
chunks' ids, and part of the history was written from them ([RUNS.md](RUNS.md)); every chunk any
source gives is checked.

**Manifests** come from the manifest host once, are checked to hash to the id asked for, and are
kept as files named by id.

## Files and ranges

A file is read as `Read + Seek` over its chunks: a seek costs nothing, and a read fetches the
chunks it overlaps that the reader does not hold, in one request to the source, and holds them
until the next read. A WAD's table of contents is at its front, so opening a WAD fetches its first
chunks only. Its changed entries are then preloaded in table order, in batches of about 32 MB of
stored bytes: the chunks of a batch come in one request to the source, which the CDN turns into one
request a bundle, and are held while its entries are read. One chunk often ends one entry and
starts the next, so each chunk is fetched once. The file id skips unchanged files, the table's
checksums skip unchanged entries, and the chunk map skips everything neither asked for.

## Formats read

WADs through `ltk_wad`; an entry's kind from its magic through `ltk_file`; bins through `ltk_meta`
and printed through `ltk_ritobin`; meshes, skeletons and textures through `ltk_mesh`, `ltk_anim`
and `ltk_texture`, `.dds` textures through `ddsfile`. Wwise banks (`bnk`, `wpk`), the inibin
family and the `r3d2sklt` skeletons before the rig resource have no ltk reader and are read by
`sync-format` itself.

An entry's kind is read from the whole entry, decompressed: an inibin has a one-byte magic and is
told by its exact length, so the first chunk of an entry is not enough. An entry the WAD's table
stores as a link is kind `link` by the table, whatever its bytes, and its bytes are the stored ones.
