# Tooling

Dependencies, lints, tests, and the oracle every writer change passes.

## Dependencies

All in `[workspace.dependencies]`; a crate names what it uses with `workspace = true`. crates.io
first, else a git dependency pinned by `rev`, never a committed path dependency.

- **LeagueToolkit crates** for every format they cover ([SOURCES.md](SOURCES.md), "Formats read").
- **ltk_ritobin is patched** to LeagueToolkit/league-toolkit#269 at `521a875` until it is released,
  with `ltk_meta`, `ltk_hash`, `ltk_primitives` and `ltk_io_ext` from the same commit, so the build
  holds one copy of each. The bin entries of the history are its `PrintCanonical` text; moving the
  pin is a writer change and passes the oracle first.
- **git** is a process, not a library: `ls-tree`, `cat-file --batch`, `fast-import`, `push`.
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
- **`tempfile`** for the sparse file a CDN read fills and for scratch repositories in tests;
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
- **No network.** CDN behaviour is tested against recorded responses replayed by a local HTTP
  server with range support: a handful of real range requests per chunk-hash scheme; a chunk found
  in two bundles with different compressed sizes, both decompressing to the same verified bytes;
  a multi-range request past the span cap, answered with the whole bundle.
- **Writers** are tested against files from the published history: a sample of entries per kind,
  each rendered from its bytes and compared with the blob the history holds.

## The oracle

The history is its own test. For a range of published builds:

1. clone or reuse the working clone, and start a branch at the commit before the range;
2. append the range's builds from their bytes, from the cache or the CDN;
3. compare each appended commit's tree id with the published commit's.

Every tree must match. A writer change, a dependency move, a new parser: each passes the oracle
over at least one patch before it lands, and over a range that crosses a WAD format or chunk-hash
change when it touches those. The run logs to `data/logs/`.

## `data/`

Gitignored, local to each machine: the working clone, the chunk cache, logs, and scratch for
experiments. Nothing in the code or the docs depends on what it holds.
