# Tooling

Dependencies, lints, tests, and the oracle every writer change passes.

## Dependencies

All in `[workspace.dependencies]`; a crate names what it uses with `workspace = true`. crates.io
first, else a git dependency pinned by `rev`, never a committed path dependency.

- **LeagueToolkit crates** for every format they cover ([SOURCES.md](SOURCES.md), "Formats read").
- **The parsers are writers.** What `ltk_file`, `ltk_wad`, `ltk_mesh`, `ltk_anim`, `ltk_texture`
  and `ddsfile` make of an entry's bytes is written into the history: `ltk_file`'s magic table is
  every entry's `kind`, `ddsfile`'s format names are a `dds` entry's `format`. Each is pinned with
  `=` to the version of the export that wrote the history, so `cargo update` cannot move it, and
  moving one is a writer change. After any `cargo update`, `cargo tree -d` lists no `ltk_*` crate
  twice.
- **`ltk_file` 0.2.11 panics on three bytes**: its JPEG pattern declares three and reads four.
  `kind_of` gives three bytes the kinds of its patterns that fit them, in its order. No entry of
  the history has three bytes.
- **ltk_ritobin is patched** to LeagueToolkit/league-toolkit#269 at `521a875` until it is released,
  with `ltk_meta`, `ltk_hash`, `ltk_primitives` and `ltk_io_ext` from the same commit, so the build
  holds one copy of each. The bin entries of the history are its `PrintCanonical` text, of objects
  read by that commit's `BinObject::from_reader`; moving the pin is a writer change and passes the
  oracle first. A "patch was not used" warning from cargo means one of the five is unpatched.
- **git** is a process, not a library: `ls-tree`, `cat-file --batch`, `fast-import`, `push`.
- **`ureq`** for downloads from Riot's CDN: blocking with no async runtime, which suits a tool whose
  parallelism is rayon's.
- **Errors**: `thiserror` in the library crates, `anyhow` in `sync-cli`.
- **`camino`** for filesystem paths (the clone, the cache, logs). A path in the history is a string:
  it names a tree entry, not a file on disk.
- **`rayon`** for the WADs of a build, read in parallel with one zstd decompressor per thread. The
  pool is sized by the run's `--jobs`, so a run can share a machine.
- **`tracing`** for logs: a span per build, WAD and entry; `tracing-subscriber` with an env filter
  writes them, and `tracing-appender` writes a run's log file. The ltk crates log through `log`;
  the subscriber's `tracing-log` feature (on by default) takes those records in when it is
  installed with `.init()`, so nothing here depends on `log` itself.
- **`serde` and `serde_yaml_ng`** for the checks before a push only: each changed YAML file loads
  into structs that reject unknown fields. Writing never goes through serde; the writers produce
  the exact text of [FORMAT.md](FORMAT.md) themselves.
- **`tempfile`** for scratch directories and repositories in tests;
  **`pretty_assertions`** in tests, so a rendered file that differs from the history's blob shows
  as a line diff.
- `Cargo.lock` is committed: this is a binary, and a run must be reproducible from a checkout.

## Toolchain and lints

Stable Rust, pinned to one release by `rust-toolchain.toml` with clippy, so a local build and CI
agree; CI installs the toolchain that file names. The workspace's `rust-version` is the same
release: the resolver prefers dependency versions whose declared Rust fits it, and a lower one
would resolve older ltk parsers than the ones that wrote the history. No rustfmt: code matches its
surroundings by hand.

The workspace lints: clippy's `correctness` and `suspicious` deny, `perf`, `style` and
`complexity` warn, `pedantic` off. A suppression is `#[expect(lint, reason = "...")]`.
`clippy.toml` forbids the filesystem calls of `std::fs` and `std::path::Path` in favour of
`fs_err`, whose errors name the path and the operation. `.editorconfig` keeps every file LF,
UTF-8, with a final newline. CI
(`.github/workflows/ci.yml`) runs `cargo clippy --workspace --all-targets --locked -- -D warnings`
and `cargo test --workspace --locked`.

## Tests

- **Inline** for a small private helper.
- **`tests/`** for anything with fixtures or several steps. Fixtures are built in the test (a WAD,
  a bin, a bank written by the test) or recorded small and committed.
- **No network.** The CDN source is tested against a local HTTP server the test runs, serving
  bundles the test writes. Range requests are to be tested against recorded responses replayed by
  such a server with range support: a handful of real range requests per chunk-hash scheme; a chunk found
  in two bundles with different compressed sizes, both decompressing to the same verified bytes;
  a multi-range request past the span cap, answered with the whole bundle.
- **Writers** are tested against files from the published history: a sample of entries per kind,
  each rendered from its bytes and compared with the blob the history holds. The game's bytes are
  not committed, so these tests (`sync-format/tests/history.rs`) are ignored by default and run on
  a machine that has them:

  ```
  CENSUS_SYNC_HISTORY=<bare clone> CENSUS_SYNC_SAMPLES=<dir> cargo test -p sync-format --test history -- --ignored
  ```

  The samples directory holds each entry's bytes in a file named for its SHA-256, and
  `entries.txt`, one `<commit> <path of the entry's own file>` per line. The same tests render
  every commit's `build.yaml`, message and time, and every `_wad.yaml` of the tip, which need no
  bytes.
- **Sources** are tested against an archive of past builds the same way
  (`sync-source/tests/archive.rs`, `CENSUS_SYNC_ARCHIVE` and `CENSUS_SYNC_HISTORY`, and
  `CENSUS_SYNC_COMMIT` for a build other than the tip): every WAD of the build read from the
  manifest and the merged bundle renders the published `_wad.yaml`, and every entry of a few WADs
  is read and renders its published files.
- **The kind audit.** The oracle reads only the entries that changed in its range, so a change to
  `kind_of` or to `ltk_file` also runs the writer tests over every tip entry of a kind that rests on
  a guess or is rare (`inibin`, `lightgrid`, `tga`, `png`, `stringtable`) and every entry with no
  kind, with a sample of each other kind.

## The oracle

The history is its own test. For a range of published builds:

1. clone or reuse the working clone;
2. for each build of the range, set a branch to its published parent and append the build from its
   bytes, from the cache, a mirror, an archive or the CDN;
3. compare the appended commit's tree and commit id with the published commit's.

Each build goes onto its published parent, so one build that differs does not hide the ones after
it; while every commit is identical, that is the same as appending the range in a row.

Every tree must match. A writer change, a dependency move, a new parser: each passes the oracle
over at least one patch before it lands, and over a range that crosses a WAD format or chunk-hash
change when it touches those. The run logs to `data/logs/`.

## `data/`

Gitignored, local to each machine: the working clone, the chunk cache, logs, and scratch for
experiments. Nothing in the code or the docs depends on what it holds.
