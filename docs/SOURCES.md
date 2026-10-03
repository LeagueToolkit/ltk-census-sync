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

| version | hash |
| --- | --- |
| 1 | SHA-512 |
| 2 | SHA-256 |
| 3 | RITO_HKDF: PBKDF2-HMAC-SHA256 over the SHA-256, empty salt, 32 rounds |
| 4 | BLAKE3 (from 16.x) |

Every chunk is checked after decompression: its hash under its file's scheme and the uncompressed
size the manifest gives. Identity is that hash alone. The same chunk can be compressed differently
in different bundles, so compressed bytes and sizes are never compared or reused across bundles.

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
kilobytes here and there across many bundles. The wanted chunks of one bundle go in one request as
a multi-range `Range` header, and the CDN answers `206 multipart/byteranges`. Past about 258 spans
it ignores the header and sends the whole bundle, which is not an error, so requests carry at most
128 spans and the response's shape is checked.

## Chunk sources

A chunk is asked for **by id**, and each source finds it its own way:

- **The cache**: chunks already fetched, by id, on local disk. Checked first; what the CDN returns
  is written back, so a chunk is downloaded once.
- **The CDN**, by the layout of the manifest being read.
- **A local merged bundle**, the form an archive of past builds keeps (rman's one bundle per part,
  with its own table from chunk id to location). Read by chunk id and its own table only, never
  with a manifest's offsets, which describe another encoding.

## Files and ranges

A file is read as `Read + Seek` over its chunks: a seek costs nothing, and a read fetches the
chunks it overlaps that are not there yet, into a sparse temporary file the size of the whole.
A WAD's table of contents is at its front, so opening a WAD fetches its first chunks only; reading
one entry fetches the chunks that cover it. The file id skips unchanged files, the table's
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
