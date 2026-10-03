# Measured runs

Facts about single runs, oldest first, each dated. The first ones were made with the local export
that wrote the history up to 16.19, before this tool, and are the baseline it starts from.

## The append proof (2026-10-03)

- **What.** The builds 16.10 to 16.19 (38) appended onto a history cut at 16.9, from a local merged
  bundle of past builds and the tip alone, then each tree compared with the history written from
  the database.
- **Result.** 38 of 38 trees identical.
- **Speed.** 984 s in all, about 26 s a build; the builds themselves 16.8 s on average, from 3.6 s
  for a build that changed no WAD to 138 s for one that rewrote 515,287 files.
- **The pause.** After each build that wrote over 300,000 files, the next started one to two
  minutes late (69 s after 324,576 files, 123 s after 515,287). The time is outside the builds'
  own work. The repository was written with zlib off, so each such build left a pack of several
  gigabytes uncompressed.

## The full history (2026-10-03)

- **What.** Every live build from 8.20 to 16.19: 755 commits, 190 tags.
- **Export.** 49 minutes, 5,566,450 blobs, 102 GB of uncompressed packs.
- **Repack.** `repack -adf --window=250 --depth=50`, zlib 9: 33 minutes, **3.80 GiB**, 10,985,990
  objects. No blob over 50 MB.
- **Tip.** 1,412,087 files: 952,405 `.yaml`, 459,678 `.rito`. The largest directory holds 404.

## Publishing (2026-10-03)

- **Push.** `main` first, then `history` in five slices sized at about 800 MB on disk each (GitHub
  refuses a push over 2 GB), then the tags: 26 minutes in all.
- **Clone.** `git clone --mirror` of the published repository: 5.89 GiB received at 16.3 MiB/s.
  GitHub sent 4,677,327 objects reused from one pack and compressed 1,834,478 on the fly, the
  objects whose delta bases arrived in a later slice of the push.

## An independent check of the published history (2026-10-03)

A separate reader, with its own strict structs that reject unknown fields and ltk_ritobin at
`521a875`, over the published `history` (`372cee97`):

- **Layout.** All 755 commits' diffs (3,792,214 adds, 4,938,801 modifies, 2,380,127 deletes): every
  path fits the layout and both bucket levels match their hashes.
- **YAML.** Every distinct blob loads: 2,143,250 entry files, 144,897 `_wad.yaml`, 755
  `build.yaml`; every hex field has its width, `links` are sorted with no repeats.
- **Bin entries.** All 3,165,803 distinct `.rito` blobs (101 GB of text) parse, each a one-entry
  bin named for its entry hash; 322 s on 24 threads.
- **Keys.** Every one of the 18,882,126 `objects` listings of builds from 10.8 on has a `.rito`
  whose text, written back to bytes, gives that `content` and `class`. At 16.19 the rebuilt bins
  match all 459,678 listings in 46,614 bins, and every bin's folder holds exactly its `objects`
  keys.
- **Against the game's own 16.19 files.** Every WAD and entry is present, every bin's `links` equal
  the game's, every object and all 261,904 wems in 10,326 banks match, and the 235 `PTCH` bins have
  neither `objects` nor a folder.
- **Found.** A bin with no objects has `links` and no `objects` key: 3,105 bins at 16.19, 716
  distinct. Documented in [FORMAT.md](FORMAT.md) rather than changed.

## The writers against the history (2026-10-03)

`sync-format`, ported without the database, against the published `history` (`372cee97`), each
entry's bytes read from the archive of past builds (`sync-format/tests/history.rs`).

- **Commits.** All 755: `build.yaml`, message and time identical.
- **WADs.** All 4,047 `_wad.yaml` of the tip render identically from their fields.
- **Entries.** 70 entries of seven WADs, every kind they hold and the bin edge cases (`PTCH`,
  links without objects, map22's bin of 18,954 entries): every file identical, 19,003 `.rito`
  among them.
- **The kind audit.** 6,679 entries of the tip: every entry of kind `inibin` (1,589), `lightgrid`
  (174), `tga` (69), `png` (33) and `stringtable` (84), every entry with no kind (4,252), and 40 of
  each other kind. `kind_of` gives each its recorded kind, and every file is identical. 486 s in a
  debug build. Two sampled entries were left out: their bytes could not be read from the archive.
- **C++ ritobin.** The 10 sample bins through `ritobin_cli -k`: every link list and all 19,003
  entries equal to the `.rito` files.
- **Found.** No entry of kind `link` and no joint parent written as an ordinal, at 16.19 or at the
  first patch of each season from 9 to 15. Both rules are in [FORMAT.md](FORMAT.md) and covered by
  tests built from synthetic bytes.

## Reading builds from the archive (2026-10-03)

`sync-source` against the published `history`, each build read from the archive of past builds:
its manifest and the merged bundle (`sync-source/tests/archive.rs`).

- **WADs.** At 9.2, 10.7, 10.8, 11.1, 12.1, 13.1, 14.1, 15.1, 15.24, 16.1, 16.5, 16.10 and 16.19
  (from 3,273 WADs at 9.2 to 4,711 at 15.24), every `.wad.client` of the manifest is a WAD of the
  commit, and its table read through the bundle renders the published `_wad.yaml`.
- **Entries.** At each of the first ten and at 16.19, every entry of `Ahri.wad.client` and
  `Ahri.en_US.wad.client` is read, its checksum is the published one, and its files are the
  published files: from 709 entries and 1,014 files at 9.2 to 6,105 entries and 8,679 files at
  16.19. 7 s a build at 9.2, 26 s at 16.19, in a debug build.
- **Chunk hashes.** Every manifest checked uses one chunking version: 3 (RITO_HKDF) from 9.2 to the
  last build of 16.3, 4 (BLAKE3) from 16.4.7461248, the first build of 16.4. None uses 1 or 2.

## The archive's misplaced chunks (2026-10-03)

`census-sync verify` over the WADs of the archive's manifests, then each bad chunk against Riot's
bundle for it.

- **16.10 to 16.19** (38 manifests): 10 of 741,397 chunks do not verify. **16.4 to 16.9** (32
  manifests): 5 of 610,672, not compared with Riot's bytes.
- **What the 10 hold.** Each bad id is the BLAKE3 of Riot's bytes for it: the manifests are right.
  The archive holds the frame of another chunk of the same bundle instead, 1 to 57 rows away, of the
  same uncompressed and compressed size, and holds that chunk under its own id as well; three bad
  ids hold one chunk's frame, two another's. No two hashes collided: of the 5,056 chunks of the six
  bundles, hashed under all four schemes, none gives a bad id or the id of the bytes held for it,
  whole or in either 32-bit half, but each chunk's own BLAKE3. A chunk was taken for a neighbour of
  its size, and a check of ids under RITO_HKDF alone, which every BLAKE3 id fails, would not see it.
- **Riot's bundles.** The 6 bundles holding the 10, whole from the CDN over https: 122 MB, 5,056
  chunks. With them in front of the archive, every chunk of the WADs of 16.15.7983109 and
  16.16.8032921 verifies (628,096).

## The oracle over 16.10 to 16.19 (2026-10-03)

- **In a row, from the archive alone.** 23 builds reproduced, trees and commit ids, then it stopped
  at 16.15.7983109 on the first chunk that did not verify. 34.7 s for 16.10.7742490 (347 WADs
  changed, 324,576 files written) with nothing cached.
- **Each build onto its published parent, the bundles in front.** 36 of 38 trees and commits
  identical, 9.3 s a build with the archive in the OS's cache. The two that differ are
  16.15.7983109 (3 paths) and 16.16.8032921 (50).
- **Found.** Those paths are 13 entries in 11 WADs whose published files were written from the
  misplaced chunks' bytes: `teemo` `956cbfd0d84d4c06` at 16.15.7983109; at 16.16.8032921, `jhin`
  `08fa46a4225477c5`, `17e6f7c39b188c03` and `f7b45a49df96babb`, `jinx` and `ruby_jinx`
  `aa9e292bc27e32e0` and `d4c60efa52e9b40b`, `milio` `1f21fbf687da55b8`, `nami`
  `56a3695d9c2dbcfc`, `riven` `525edc9d3d07c6f7`, `taliyah` and `map11` `fe4f2aa2c99643d9`. Riot's
  bytes hold other objects. The published files carry over into the commits after, until each entry
  changes.

## The fix of 16.10 to 16.19 (2026-10-03)

- **What.** `census-sync rebuild 16.9 38` onto a branch of its own, the six bundles in front of the
  archive: 354 s.
- **Result.** The 23 builds to 16.14.7949266 are the published commits. From 16.15.7983109 on, 15
  commits are new; each differs from the published one in the misplaced chunks' entries while the
  published files of them are wrong: 3 paths through 16.15, 50 at 16.16, 3 from 16.17. At 16.19 one
  entry remains, `nami` `56a3695d9c2dbcfc`. The tags 16.15 to 16.19 move.
- **Published** (2026-10-03, by the user): `history` replaced from 16.15 on, its tip `39fa90b1`, and
  the tags 16.15 to 16.19 moved; the repository was private, so no other clone held the old commits.

## The archive audit (2026-10-03)

- **What.** `census-sync verify --all-files` over every RMAN build of the history, 9.2 to 16.19
  (744 manifests, the RADS builds of 8.20 to 9.1 left out): every chunk of every file, read from
  the archive alone. 4,825,753 chunks, 1,182 s.
- **Result.** 15 chunks do not verify, all of them BLAKE3 ids in WADs, in 8 bundles. None of the 674
  manifests of chunking version 3 (RITO_HKDF, 9.2 to 16.3) has one, and no file but a WAD has one.
  Five are first used by 16.4.7461248, the first build with BLAKE3 ids.

| chunk | bundle | first build | size | file |
| --- | --- | --- | ---: | --- |
| `263a0daede511319` | `C04676802EA1EC5A` | 16.4.7461248 | 32 | `Taric.ro_RO.wad.client` |
| `a888ebf03990f8c4` | `C04676802EA1EC5A` | 16.4.7461248 | 32 | `Taric.ro_RO.wad.client` |
| `681e1bbfee666347` | `C04676802EA1EC5A` | 16.4.7461248 | 32 | `Taric.tr_TR.wad.client` |
| `86551fd68445611d` | `C04676802EA1EC5A` | 16.4.7461248 | 32 | `Taric.tr_TR.wad.client` |
| `8594fe8cae13abf5` | `B9640EB24AE08202` | 16.4.7461248 | 1995 | `Chogath.wad.client` |
| `875d82ed8116fb9e` | `8E5691EADF0E1714` | 16.15.7983109 | 715 | `Teemo.wad.client` |
| `2ec41a77f91276aa` | `546A29A0F5A25391` | 16.16.8032921 | 1064 | `Taliyah.wad.client` |
| `589b84454d8a4816` | `2D29125CE022A1B4` | 16.16.8032921 | 325 | `Jhin.wad.client` |
| `ee5a18111dd664b7` | `2D29125CE022A1B4` | 16.16.8032921 | 325 | `Jhin.wad.client` |
| `f5f6d414758c5f32` | `2D29125CE022A1B4` | 16.16.8032921 | 325 | `Jhin.wad.client` |
| `59882ebfd07cba35` | `2D29125CE022A1B4` | 16.16.8032921 | 283 | `Jinx.wad.client` |
| `6ae22b8cbe897a46` | `2D29125CE022A1B4` | 16.16.8032921 | 283 | `Jinx.wad.client` |
| `8c8cfbc44a039927` | `65FD50F448174293` | 16.16.8032921 | 114 | `Nami.wad.client` |
| `bccec0284fe115e1` | `3B96C3FD48CCF283` | 16.16.8032921 | 1766 | `Riven.wad.client` |
| `fbacf206d7de9ee3` | `A65363806117E40B` | 16.16.8032921 | 948 | `Milio.wad.client` |

Sizes are uncompressed, in bytes; files are under `DATA/FINAL/Champions/`. The ten from 16.15 on
are the ones of the oracle above. Whether the five of 16.4.7461248 left wrong files in the
published history was not checked.

## The CDN as a fallback (2026-10-03)

- **What.** `census-sync oracle` on 16.15.7983109 alone, onto its published parent, from the archive
  with an empty mirror and `--cdn`.
- **Result.** The archive's bad chunk of the build failed its check, its bundle `8E5691EADF0E1714`
  was downloaded over https into the mirror, and the build came out as the commit the six bundles
  gave before (`8cdff728e1f8`). 38.2 s, the download included.

## The oracle over 16.4 to 16.9 (2026-10-03)

- **What.** The 32 builds of 16.4 to 16.9, each onto its published parent, from the archive behind
  the mirror, with `--cdn`.
- **Result.** 32 of 32 trees and commits identical, 7.1 s a build, and no bundle downloaded: the
  five bad chunks first used by 16.4.7461248 were never read. Their entries kept the checksums of
  earlier builds, whose chunks verify, so neither the export nor an append read them. The
  published history from 16.4 to 16.9 is correct, and the fix of 16.10 to 16.19 is the whole fix.
