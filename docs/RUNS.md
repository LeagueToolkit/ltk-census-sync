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
