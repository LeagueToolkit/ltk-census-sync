# AGENTS.md

Guidance for coding agents in ltk-census-sync, the maintainer tool that appends each new live
build of League of Legends to the census history, a git repository of the game's data with one
commit per build. The design is in `docs/` ([docs/INDEX.md](docs/INDEX.md)). Read the doc for the
area you change before changing it, and update it in the same change.

## Working here

- **Checks.** `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace` come back clean. CI runs both.
- **No `cargo fmt`.** Match the surrounding code by hand.
- **The format is frozen.** What the history holds is format 2 ([docs/FORMAT.md](docs/FORMAT.md)),
  and every file is compared byte for byte with the published history. A change to any writer
  passes the oracle ([docs/TOOLING.md](docs/TOOLING.md)) before it lands. Adding an optional field
  or a family is still format 2; changing or removing anything is format 3, and is the user's
  decision, not a refactor.
- **Output is deterministic.** No clock, no machine state, no unordered map reaching a file or a
  commit. A commit's date is the build's own, never the time of the run.
- **Nothing is pushed by accident.** Development and tests push to a stand-in remote made for the
  purpose. Pushing to the real history is an explicit operation the user runs or asks for. Never
  commit or push unasked, in this repository or the history.
- **Stop rather than push a partial commit.** A format that does not parse, a chunk that does not
  verify, a download that fails midway: the run stops and reports, and the tip stays as it was.
- **Riot's CDN is not a test fixture.** Tests replay recorded responses. Real requests go through
  the chunk cache, a chunk is fetched once, and ranges are batched ([docs/SOURCES.md](docs/SOURCES.md)).
- **Tests.** A small check of a private helper may sit inline. Anything with fixtures or several
  steps goes in the crate's `tests/` directory; fixtures are built in the test or recorded small.
- **League formats through `ltk_*` crates** where one exists. Dependencies land in
  `[workspace.dependencies]`: crates.io first, else a git dependency pinned by `rev`, never a
  committed path dependency.
- **Lints.** Suppress one with `#[expect(lint, reason = "...")]`, never a bare `#[allow]`. A
  panic states what failed and with what value.
- **Filesystem calls go through `fs_err`** (`use fs_err as fs;`), never `std::fs`: its errors name
  the path. `clippy.toml` enforces it.
- **Logging is `tracing`**, with a span per build, WAD and entry, so an error in a log names where
  it happened. Long runs (an append of many builds, the oracle over a patch range) log to
  `data/logs/`, so their progress can be read while they run.

## Writing

Plain, literal language in replies, docs, comments and commit messages.

- One name for one thing. The short common word: use, not utilize or leverage; start, not
  initiate. No marketing adjectives (robust, seamless, powerful).
- Active voice, one fact per sentence. No metaphor or emphasis that carries no information:
  "adjust the setting", not "turn the dial".
- Keep technical terms where they are precise.

**Docs have a lifecycle.** A design doc in `docs/` states what is true and is edited in place:
when the code departs from it, the section is rewritten, not appended to. It carries no
`now`, `currently`, `used to` or `no longer`, and no story of what was tried. Measurements are
facts about one run and go, dated, in `docs/RUNS.md`. A decision the user made keeps its date
beside it. History is `git log`. Docs name no local path.

## Comments

**A comment says why, not how.** The code says what it does. A comment carries what the code
cannot: the reason for a choice, a constraint from the game's data or Riot's servers, a
workaround and its cause, a magic value. The test is whether deleting it would let a reader change
the code and break something.

```
Bad   // Sort the links.
Good  // A bin's links are not kept in their stored order, so two bins that list the same links
      // differently write the same file.

Bad   // Batch the ranges in groups of 128.
Good  // Past about 258 spans the CDN answers with the whole bundle instead of an error.
```

- **Most code needs none.** Clear names, small functions and named constants come first. Strip
  comments that narrate the next line.
- **Module (`//!`)**: what the file is for and the design behind it, for a person working inside
  it. One or two lines when there is no design to explain.
- **Declaration (`///`)**: for the caller. Every public item has one. The first line names the
  thing, a noun phrase or one declarative sentence in the domain's words: `/// A WAD entry's own
  file.`, not `/// This function returns...`. A second paragraph only for what a caller needs and
  the signature cannot show.
- **Body**: only to split a long function into steps, or for a reason the code cannot show.
- **A comment is smaller than the code it explains.** When the reason needs a page, it belongs in
  `docs/`, and the comment names the file and section:
  `` //! `docs/FORMAT.md`, "A bin's entries". ``
- **Evidence is a fact worth keeping.** A count from the history, the build a field changed at,
  the response the CDN gave: these are why the code is as it is.
- **Timeless.** No "used to", "the old X", "now", and no retelling of the change. That belongs in
  the commit. The exception is a workaround whose only reason is past behaviour someone would
  otherwise restore.
- **Self-contained.** A comment makes sense in a clean checkout: it does not lean on `data/` or
  other gitignored files.
- Third person, present tense, no `we` or `you`. Drop `note that`, `simply`, `just`.

## Commits

One conventional-commit subject line. No body, no trailers, no `Co-Authored-By`. The scope is
the crate without its prefix (`format`, `source`, `history`, `cli`) or, outside one, the area
(`docs`, `ci`, `workspace`).

**A subject names the change, it does not describe it.** A plain verb and a domain noun phrase,
about three to six words, in the codebase's vocabulary. No contrastive clause, no mechanism.

```
Bad   sync: a WAD whose file id and tags did not change is carried over without reading its table
Good  feat(history): carry unchanged WADs

Bad   source: ranges are batched at 128 because the CDN sends the whole bundle past 258
Good  fix(source): batch multi-range requests
```

A test name stays a sentence: it is the only place saying what the case is.
