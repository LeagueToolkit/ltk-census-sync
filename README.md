# ltk-census-sync

The maintainer tool for the League of Legends census history: for each new live build of the game
client, it appends one commit to the history repository, computed from that repository's tip and
the new build's own bytes. Reading the history needs only git; this tool is for keeping it up to
date.

## The history

[ltk-census-history](https://github.com/LeagueToolkit/ltk-census-history) holds every live build
of the Windows game client from patch 8.20 (October 2018), **one commit per build and one file
per fact**: a WAD, a WAD entry, a bin entry. A patch reads as a diff. It is used to track how the
game's data changes from patch to patch and to help port mods across patches, and it may serve
data mining.

What it holds and in what exact form is format 2, frozen: [docs/FORMAT.md](docs/FORMAT.md). Every
tree also carries `census.yaml` and a JSON Schema per kind of YAML file, so a checkout says how to
read itself.

The builds up to 16.19 were written by a separate, local export from a database of past builds
(8.20 to 9.1 read from Riot's older RADS distribution, the rest from an archive of RMAN
manifests and bundles). From 16.19 on, the history is kept up by appending, and the history itself
is the only state: no database.

## What a run does

1. **Find** the live builds the history does not have yet.
2. **Plan** each one against the tip: a WAD whose file id and tags are unchanged carries over
   whole; in a changed WAD, an entry whose checksum is unchanged keeps its files.
3. **Fetch** only what is new: the changed WADs' tables of contents, then the bytes of the changed
   entries, as chunks from a local cache or Riot's CDN by byte range.
4. **Render** each new entry's files from its bytes, with the same writers that produced the
   history.
5. **Commit** on top of the tip, check the result, and **push**.

The details: [docs/APPEND.md](docs/APPEND.md) for the commit, [docs/SOURCES.md](docs/SOURCES.md)
for builds and bytes, [docs/OPERATIONS.md](docs/OPERATIONS.md) for running it on a schedule.

## Running it locally

It needs git and Rust; `rust-toolchain.toml` names the release, and rustup installs it on the
first build. Everything a run keeps is under `data/`, which git ignores.

Once:

```sh
git clone https://github.com/LeagueToolkit/ltk-census-sync.git
cd ltk-census-sync
cargo build --release

# the history, as a bare clone to append to (about 6 GiB)
git clone --bare https://github.com/LeagueToolkit/ltk-census-history.git data/history.git

# the manifest list, with master at the list commit that added the tip's build
git clone https://github.com/Morilli/riot-manifests.git data/riot-manifests
tip=$(git --git-dir=data/history.git show history:build.yaml | sed -n 's/^version: "\(.*\)"/\1/p')
added=$(git -C data/riot-manifests log --diff-filter=A --format=%H -1 -- "LoL/NA1/windows/lol-game-client/$tip.txt")
git -C data/riot-manifests reset --hard "$added"
```

Each time:

```sh
# what has shipped since the tip: one line per build, oldest first
target/release/census-sync status --repo data/history.git

# append each new build in that order; its version comes from the list and its date from the CDN
target/release/census-sync append --repo data/history.git <manifest>

# check every commit the remote does not have yet
pushed=$(git --git-dir=data/history.git ls-remote origin refs/heads/history | cut -f1)
target/release/census-sync check --repo data/history.git "$pushed"

# push, tags too, then move the list's checkout past the builds just appended
git --git-dir=data/history.git push origin history --tags
git -C data/riot-manifests merge --ff-only origin/master
```

The first build of a new patch tags the tip with the patch before it, which is that patch's newest
build; a tag is set once and never moves. An append downloads the manifest and the chunks of the
entries that changed: a few megabytes for a
hotfix, one to four gigabytes for a patch, kept in `data/chunks/` so a run that stops and is run
again downloads nothing twice.

## Status

It does what a run needs, run by hand ([Running it locally](#running-it-locally)): `status` finds
the live builds the history lacks, `append` writes each as a commit from the build's own bytes,
and `check` checks the commits before they are pushed. The writers reproduce the published history
byte for byte, the builds of 16.10 to 16.19 append as published, and those of 16.18 do with Riot's
CDN as the only source ([docs/RUNS.md](docs/RUNS.md)).

Not written, since a run nobody watches needs them and a run by hand does not: `census-sync push`
(the branch and its tags), `census-sync run` (all of the above in one command), and a schedule to
run it on, a NAS or CI ([docs/ROADMAP.md](docs/ROADMAP.md)).

## Layout

```
crates/sync-format    format 2: file names, YAML writers, bin entries as ritobin text, facts from bytes
crates/sync-source    live builds, RMAN manifests, chunks from a cache or the CDN, files from chunks
crates/sync-history   the history repository: the tip, the builds it holds, the next commit, checks
crates/sync-cli       the census-sync binary
docs/                 the design, one file per area
```

