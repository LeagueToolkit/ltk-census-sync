# Operations

Running census-sync: the clone it works in, what a run does end to end, where it runs, and what
happens when something fails. The commands are planned ([ROADMAP.md](ROADMAP.md)); this states how
they are meant to behave.

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
| `census-sync status` | the tip, and the live builds the history lacks |
| `census-sync append <manifest>...` | append those builds to the local `history`; no push |
| `census-sync check` | the checks of [APPEND.md](APPEND.md), "Before a push", on commits not yet pushed |
| `census-sync push` | push `history`, its tags and notes to the configured remote |
| `census-sync run` | fetch, status, append every new build in arrival order, check, push |
| `census-sync oracle <commit> <count>` | re-append `count` builds after `commit` and compare their trees with the published ones |

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

A push is `history` (fast-forward only, never forced), then the new tags, then
`refs/notes/realms`.

## When something fails

The run stops and exits non-zero, `history` is reset to the tip it started from, and nothing is
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
data/chunks/       the chunk cache, by chunk id
data/logs/         one log per run
```

The chunk cache grows by what each build changed; how much to keep is open
([ROADMAP.md](ROADMAP.md)).
