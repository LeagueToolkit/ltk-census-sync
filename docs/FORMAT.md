# Format 2

What the census history holds and the exact bytes it holds it in. The format is frozen
(2026-10-03, after an independent reader checked the published history, [RUNS.md](RUNS.md)): every
writer in `sync-format` reproduces it byte for byte, and the oracle ([TOOLING.md](TOOLING.md))
checks that against the published history.

**Evolution** (decided 2026-10-02): adding an optional field or a family is still format 2, and a
reader ignores what it does not know; changing or removing anything is format 3. `census.yaml`
carries the format number, and a reader checks it before parsing. Added so far (decided 2026-10-04,
for a reader that ports mods without a game install): an `anm` entry's `clip`, and a bank's
`header` and `objects`. Every commit holds them, from the first.

## Branches and commits

- **`main`**, the default branch, holds only `README.md` and `LICENSE` (`sync-format`'s `main/`),
  so a plain clone checks out two small files. It shares no commit with `history-v2`; its commits
  change those two files alone, on top of each other.
- **`history-v2`** holds the builds, one commit per build. Its first commit adds `.gitattributes`
  (`* -text`): every file is LF and full of hashes, and a checkout that rewrote line endings would
  corrupt them.
- **`history`** holds the same builds to 16.19.8207193 as first published, without an `anm`
  entry's `clip` and a bank's `header` and `objects`. Nothing is appended to it.
- **A commit** is authored and committed by `census <census@localhost>` at midnight UTC of the
  build's date (`+0000`). Its message is the build's version, a blank line, then trailers:

  ```
  16.19.8207193

  Census-Patch: 16.19
  Census-Manifest: ad6896ce5cf475a1
  Census-Source: rman
  Census-Realms: BR1 EUN1 EUW1 JP1 KR LA1 LA2 ME1 NA1 OC1 RU SG2 TR1 TW2 VN2
  ```

  `Census-Manifest` is the manifest id in 16 hex, `Census-Realms` the live realms that shipped the
  build, sorted, space-separated. So the same build appended twice makes the same commit id.
- **No tags** (decided 2026-10-04). A commit's message and `build.yaml` name its build and its
  patch, so `git log --grep 'Census-Patch: 16.19'` finds a patch's builds.
- **Order.** The builds up to 16.19 are in patch order: season, patch, build number, manifest id.
  Appended builds land in the order they arrive ([APPEND.md](APPEND.md)).

## Layout

```
.gitattributes                                        * -text
census.yaml                                           the format, the kinds of file, their schemas
schema/<name>.schema.json                             a JSON Schema per kind of YAML file
build.yaml                                            this build
files/<manifest path>/_wad.yaml                       one WAD
files/<manifest path>/xx/<hash>.yaml                  one WAD entry, and what its bytes say
files/<manifest path>/xx/<hash>.bin/yy/<entry>.rito   one entry of a bin, as ritobin text
```

**Every file is a fact about one WAD or one WAD entry and depends on nothing else.** An entry's
files are a function of its bytes; a WAD's `_wad.yaml` of its header and the manifest's line for
it. A WAD that did not change is a directory that did not change, and the next build is written
from the tip and the new bytes alone ([APPEND.md](APPEND.md)).

**Names are stable identities, never content.** git sorts objects by a hash of their path before
its delta search, so every version of one file lands together and deltas against the others. A
file named for its content would scatter its versions.

- `<manifest path>`: the file's path in the manifest, lowercased (`data/final/champions/ahri.wad.client`).
  The name says which locale a WAD is; the manifest's tags in `_wad.yaml` say which realms consume
  it.
- `xx/<hash>`: the entry's path hash (XXH64 of its lowercased path) as 16 hex, in the directory of
  its first byte (`path_hash >> 56`, 2 hex). One level: the median WAD holds a dozen entries and
  the largest about 90,000, some 350 a directory.
- `<hash>.bin/yy/<entry>.rito`: a bin's entry, named for its entry hash as 8 hex, in the directory
  of its first byte (`entry_hash >> 24`). The full hash is the file's name so that it is the
  entry's key in the bin's `objects`.

`census.yaml` names the schema for each path pattern:

```yaml
format: 2
description: "League of Legends builds as a git history, one commit per build; written by ltk-census (census export git). Hashes are hex; naming them is another repository's job."
files:
  build.yaml: "schema/build.schema.json"
  files/**/_wad.yaml: "schema/wad.schema.json"
  files/**/??/????????????????.yaml: "schema/entry.schema.json"
  files/**/??/????????????????.bin/??/????????.rito: "ritobin"
```

## The YAML

A strict subset, so that every YAML reader loads it the same and a diff keeps its shape:

- **Every string double-quoted, every integer plain.** A bare `8.20` loads as a float,
  `2026-09-21` as a date, `00012345` as an integer, `NO` as false. Inside quotes, `"` and `\` are
  escaped and a control character is `\xHH`.
- **Block style only.** The one flow form is `[]` for an empty list.
- **One space of indentation per level.** A sequence item's further keys align with the character
  after its `- `, two deeper than the dash.
- **Fixed key order per kind of file; records sorted by their key; hex lowercase.** A field that is
  not known is absent.
- **Every number is unsigned.** Every hash and id is written as hex of its unsigned value, every
  count as a plain non-negative integer.

## Hashes

**A SHA-256, or a prefix of one, is hex in digest order; an XXH value is its integer in hex.**

- SHA-256: an entry's `sha256` and an object's `content`, whole (64 hex); an object's `norm` and a
  wem's `hash`, their first eight bytes (16 hex).
- XXH: path hashes (XXH64), a texture's `top` and a mesh's `positions` and `rounded` (XXH3),
  `checksum` (XXH64 or XXH3, by WAD version).
- FNV-1a 32: bin entry, class and field names (8 hex), as the game stores them. Naming hashes is
  not this format's job.

## build.yaml

```yaml
version: "16.19.8207193"
patch: "16.19"
manifest: "ad6896ce5cf475a1"
source: "rman"
date: "2026-09-21"
legacyBins: false
realms:
 - "BR1"
 - "EUN1"
```

- `version`: the client's version; `patch`: season and patch.
- `manifest`: the RMAN manifest id, 16 hex. `source: "rman"` says it is one.
- `date`: the day the manifest was published (its `Last-Modified`), UTC.
- `legacyBins`: whether the build's bins use the property-kind numbering of builds before 10.8.
- `realms`: the live realms that shipped the build, sorted. An append writes `NA1`, the one realm
  census-sync follows ([SOURCES.md](SOURCES.md), "Live builds"); the builds up to 16.19 list the
  realms the export knew to have shipped them, which for 209 of them is `NA1` alone.

8.20 to 9.1 are `source: "rads"`, read from Riot's older RADS distribution: no RMAN manifest exists
for them, so `manifest` is XXH64 of `rads/<realm>/<solution>/<version>`, `solution`, `release` and
`exe` follow `source`, `date` is the client executable's link time, and the realms are the one
`rads:live`, in `build.yaml` and in `Census-Realms`. An append writes RMAN builds only.

`build.yaml` changes in every commit, because no two builds share a manifest; a build that touches
no content is still a commit.

## A WAD: `_wad.yaml`

```yaml
version: "3.4"
fileId: "27f7f49ff6d41bc4"
tags:
 - "en_AU"
 - "en_GB"
```

- `version`: the WAD format's `major.minor`.
- `fileId`: the manifest's id for this version of the file, 16 hex. It changes whenever the WAD's
  bytes do, so `_wad.yaml` changing means the WAD changed.
- `tags`: the manifest's tag set for the file, sorted and deduplicated, `none` dropped; `[]` for
  none.

## An entry: `xx/<hash>.yaml`

```yaml
sha256: "fce40504131784b234f2ef8fb9aa873fef77ea8bd3865802b1db2effd9e0c05e"
checksum: "f77de45e4dc943f9"
kind: "skl"
skeleton:
 influences: 26
 joints:
  - name: "VFX_1_BabyFox_Root"
  - name: "VFX_1_BabyFox_Chest"
    parent: "VFX_1_BabyFox_Root"
```

In this order:

- `sha256`: of the entry's bytes, decompressed. The same anywhere means the same bytes.
- `checksum`: the checksum the WAD's table holds for the entry, 16 hex; absent in a WAD without
  them. An append compares it to decide whether the entry changed.
- `kind`: what the bytes are by their magic (`bin`, `tex`, `dds`, `skl`, `skn`, `anm`, `bnk`,
  `wpk`, `link`, ...); absent when unknown.
- `links`: a bin's links (the other bins it depends on), sorted and deduplicated, `[]` for none;
  for a bin that splits only.
- Then what the bytes say, under one key for the kind, when they parse. A section with nothing in
  it is not written: a bin with no objects has `links` and no `objects` key (and no
  `<hash>.bin/`), unlike `links`, which is `[]` when empty. A reader takes a missing `objects` as
  none.

**`skeleton`** (`skl`): `influences`; `name` and `asset` when the skeleton stores them; `joints` in
stored order, each with `name` and, except for a root, `parent`: the parent's name, or its ordinal
(an integer) when another joint shares that name. A name rather than an ordinal keeps a joint
inserted mid-list one record of diff.

**`mesh`** (`skn`): `vertexType`, `vertices`, `indices`, then `submeshes` in order, each with
`name`, `vertices`, `indices`, `positions` (XXH3 of the positions as stored) and `rounded` (XXH3 of
the positions rounded to 1/16 unit and sorted).

```yaml
mesh:
 vertexType: "Basic"
 vertices: 48
 indices: 180
 submeshes:
  - name: "lambert1"
    vertices: 48
    indices: 180
    positions: "66216ca23130bbcd"
    rounded: "72175e93b32a1d95"
```

**`clip`** (`anm`): `joints`, the joints the clip animates, each the 32-bit hash the clip stores
for it as 8 hex, in stored order, `[]` for none. Both clip forms hold the list: the compressed one
and versions 4 and 5 of the uncompressed one as hashes, version 3 as joint names, which are hashed
with ELF as the game hashes a joint's name. Only the list is read, so a clip whose frames would not
parse still has it.

```yaml
clip:
 joints:
  - "0a1b2c3d"
  - "1f2e3d4c"
```

**`texture`** (`tex`, `dds`): `format` (`bc1`, `bc3`, `bgra8`, ..., or the DDS one, `dxt1`,
`dxt5`, ...), `width`, `height`, `mips`, and `top`, XXH3 of the top mip's stored bytes.

**`bank`** (`bnk`, `wpk`): `version`; `bankId` when the bank has one; `header`; `media`, by wem
id, each with its `size` and `hash` (the first eight bytes of the SHA-256 of the wem's bytes); for
an events bank `events`, by event id, each listing the wem ids its play actions reach through the
bank's hierarchy (`[]` for none); and `objects`. Each media and event record ends with a blank
line.

`header` and `objects` are what a bank with a `HIRC` section stores, kept as bytes so that a reader
can write the bank again: `header` is the body of its `BKHD` section as hex, and `objects` is every
`HIRC` object in stored order, each with its `id` (8 hex), its `type` (the type byte, a number) and
its `body`, the object's bytes after its id, as hex. A `HIRC` object is a type byte, a 32-bit size
and that many bytes, which start with the id; an object too short to hold an id has no `id`, and
its `body` is all of its bytes. An object the bank repeats is listed each time. A body holds
settings and ids, never audio. A bank with no `HIRC` section has neither key.

```yaml
bank:
 version: 145
 bankId: "70529222"
 header: "91000000222905703e5d701710000000fa00000000000000f07d3c6de0cedd54"
 media:
  "0001f14f":
   size: 8988
   hash: "34d35b3a89cdc6aa"

 events:
  "0a0b0c0d":
   - "0001f14f"

  "0e0f1011": []

 objects:
  - id: "0a0b0c0d"
    type: 4
    body: "01a1b2c3d4"
  - id: "d4c3b2a1"
    type: 3
    body: "03040d0c0b0a00"
```

**`objects`** (`bin`): every object the bin holds, by entry hash in unsigned order, whether or not
it parses, each with its `class` (8 hex), `content` (SHA-256 of the object's bytes as the bin
stores them, from its entry hash to its end) and `norm` (the first eight bytes of the SHA-256 of
those bytes after the entry hash, so the same object under another entry name has the same norm).
An entry hash the bin repeats keeps its first object. These are keys over the binary bytes, which
no reader of the ritobin text can recover.

```yaml
links:
 - "DATA/Characters/Jade_Ahri/Jade_Ahri.bin"
objects:
 "3537586f":
  class: "9b67e9f6"
  content: "6f04c4169271542b4c7c784ba3fcee73cc2e9ffc39150d3e36e90818d420a411"
  norm: "7d2458dd9f78f93f"
```

## A bin's entries: `<hash>.bin/yy/<entry>.rito`

One entry of the bin: the `#PROP_text` document of a bin holding that one entry, version 3, no
links, written as C++ ritobin writes it, so `ritobin_cli -k` on that one-entry bin gives the same
bytes:

```
#PROP_text
type: string = "PROP"
version: u32 = 3
linked: list[string] = {}
entries: map[hash,embed] = {
    0xb08673e8 = 0x45cd899f {
        0xecf1c6bc: string = "Nami_Base_Q_cas"
    }
}
```

- Four spaces of indentation; hashes `0x%08x`, a `file` `0x%016x`; integers decimal; floats as
  `std::to_chars` writes them: the shortest digits that round-trip, an exact tie to the even
  digit, fixed or scientific whichever is shorter (`25`, `0.5`, `1e+05`), and an integer past its
  shortest digits as its exact value (2^36 is `68719476736`); strings with C escapes
  (`\t \n \r \b \f \\ \"`), `\xHH` below 0x20, every other byte as it is.
- The text is ltk_ritobin's `PrintCanonical` (LeagueToolkit/league-toolkit#269, pinned at
  `521a875` until released), which writes C++ ritobin's text.
- A file depends on its object's bytes and class alone, so one object is one blob in every bin and
  build that holds it.
- An object of a build before 10.8 is read by the legacy property-kind numbering, or by the other
  numbering when that fails, and written as the same modern text.
- An object that does not parse has no file and is still listed in `objects`.

**The whole bin** is its entry's `links` as the header's `linked`, then the lines between
`entries: map[hash,embed] = {` and the closing `}` of every file under `<hash>.bin/`, in path
order, which is unsigned entry hash order. That is `ritobin_cli -k`'s text for the bin, byte for
byte. Its stored order, its version and its link order are not kept.

## Not in the history

- **The contents.** Facts about bytes, not the bytes: no textures, meshes, audio or data files.
- **`PTCH` bins** (overrides): such an entry has its `sha256` and `kind: "bin"`, and no `links`,
  `objects` or `<hash>.bin/`.
- **Names** of hashes. They are learned over time, not shipped per patch, and belong to the
  community hash lists.
- **PBE.** Live realms only.
- **Builds before 8.20**, whose game data was partly loose files.
