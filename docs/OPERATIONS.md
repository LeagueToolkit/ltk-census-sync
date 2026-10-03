# Operations

Running census-sync: the clone it works in, what a run does end to end, where it runs, and what
happens when something fails. `append` and `oracle` are written; the other commands are planned
([ROADMAP.md](ROADMAP.md)), and this states how they are meant to behave.

## The working clone

A persistent bare clone of the history, which every run appends to and pushes from. An append reads
the tip's tree and, for each changed WAD, its entries' own files: thousands of blobs a build, read
in one `git cat-file --batch`. A partial clone (`--filter=blob:none`) would fetch each missing blob
on its own round trip, so the clone is full. Cloned from GitHub it is about 6 GiB
([RUNS.md](RUNS.md)); a local `git repack -adf --window=250 --depth=50` brings it to about 4.

Pushed commits are appended on top of what the remote has, so the clone and the remote never
diverge: a run starts by fetching `history`, and stops if the remote has moved somewhere the clone
cannot fast-forward to.

## Commands

| command | does |
| --- | --- |
| `census-sync status` | fetch the manifest list, and print the tip and the live builds the history lacks, in arrival order, each with its version, manifest and date |
| `census-sync append <manifest>` | append one build to the local `history`, its version from the manifest list and its date from the CDN unless given; no push |
| `census-sync check <commit>` | the checks of [APPEND.md](APPEND.md), "Before a push", on each commit after `commit`, the last one pushed; prints each problem |
| `census-sync push` | push `history`, its tags and notes to the configured remote |
| `census-sync run` | fetch, status, append every new build in arrival order, check, push, and move the list's checkout to the fetched commit |
| `census-sync oracle <commit> <count>` | re-append `count` builds after `commit` on a branch of its own, and compare each tree and commit with the published one |
| `census-sync verify <manifest>...` | check every chunk the WADs of those manifests use, and list each that does not verify with the bundle its manifest places it in; for a mirror or the cache (`--offline`), since it asks for one chunk a request |
| `census-sync rebuild <commit> <count>` | re-append `count` builds after `commit` in a row on a branch of its own, from their bytes and their published facts, and list where each tag of the range would move; the history as its bytes write it, for a fix of the published history |

The commands read manifests from `--manifests` and chunks from Riot's CDN by range, through the
chunk cache (`--cache`); a manifest or a chunk not there is downloaded, once
([SOURCES.md](SOURCES.md), "Chunk sources"). A mirror of whole bundles (`--mirror`) is read first
when given. `--offline` reads the manifests, the mirror and the cache only. `--cdn-host` names one
host that serves manifests and bundles at Riot's paths, such as a mirror served over HTTP, in place
of Riot's. One run holds the cache at a time.

## A scheduled run

`census-sync run` from cron, a few times a day (decided 2026-10-02: a FreeBSD jail on a NAS; Rust
builds there and no runner is needed). The push uses a deploy key with write access to the history
repository only.

A run is idempotent: the history is the state, so a run that finds nothing new does nothing, and a
run that failed leaves the tip where it was for the next one to retry.

A GitHub-hosted job is possible too, since nothing needs a database: a runner needs the clone, the
new manifests and the changed bytes over HTTP ranges. Its disk (about 14 GB free) holds the clone
and a build's chunks.

## Pushing

The real remote is set explicitly and is never the default. Development pushes to a **stand-in
remote**: a local bare clone of the history, made for the purpose, so a push to the wrong place
lands nowhere that matters.

A push is `history` (fast-forward only, never forced), then the new tags.

## When something fails

The run stops and exits non-zero, `history` stays at the tip it started from, and nothing is
pushed. Causes it stops for:

- a file the build ships that a writer cannot parse in a way it has not seen before;
- a chunk that does not verify, or a download that fails after its retries;
- a check before the push that does not pass;
- a remote that has moved in a way the clone cannot fast-forward to.

The log says which build, which WAD and which entry. cron reports the exit status. Fixing the cause
and running again is the whole recovery.

## Local state

```
data/history.git   the working clone
data/riot-manifests/  a clone of the manifest list, at the last commit appended (`--list`)
data/manifests/    manifests by id, as downloaded, and the day each was published
data/chunks/       the chunk cache: frames by hash scheme, uncompressed size and chunk id
data/cdn/          whole bundles at their CDN paths, a mirror, when one is kept
data/logs/         one log per run
```

The chunk cache grows by what each build changed, 2 to 4 GB a patch; how much to keep is open
([ROADMAP.md](ROADMAP.md)).
