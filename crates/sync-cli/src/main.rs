//! `census-sync`: finds the live builds the history lacks, appends each as a commit, checks the
//! result and pushes it (`docs/OPERATIONS.md`). The commands not written yet are planned in
//! `docs/ROADMAP.md`.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use clap::{Args, Parser, Subcommand};
use rayon::prelude::*;
use sync_format::{legacy_bins, BuildFacts};
use sync_history::{append, build_facts, check, manifests, Appended, Git};
use sync_source::{
    BundleMirror, Cdn, CdnSource, ChunkCache, ChunkRef, ChunkSource, Downloaded, Layers, Manifest, ManifestList, LIVE_REALM,
};

/// The branch that holds the builds (`docs/FORMAT.md`, "Branches and commits").
const HISTORY: &str = "history-v2";

#[derive(Parser)]
#[command(about = "Append new live builds of League of Legends to the census history", version)]
struct Cli {
    /// Worker threads for the WADs of a build; the machine's parallelism by default.
    #[arg(long, global = true)]
    jobs: Option<usize>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The tip, and the live builds the manifest list has added that the history lacks, in the
    /// order they arrived.
    Status(StatusArgs),
    /// Append one build to the local history; no push.
    Append(AppendArgs),
    /// Check the commits after a pushed one, before they are pushed: every file they add or change
    /// by its kind, each message, and each tree against its build's manifest. Prints each problem.
    Check(CheckArgs),
    /// Re-append builds of the history, each onto its published parent, and compare each tree and
    /// commit with the published one.
    Oracle(OracleArgs),
    /// Check every chunk the WADs of some manifests use: its bytes decompress to its size and hash
    /// to its id. Prints each bad chunk with the bundle its manifest places it in.
    Verify(VerifyArgs),
    /// Re-append builds of the history in a row onto a branch of their own, from their bytes and
    /// their published facts: the history as those bytes write it. Prints where a tree differs
    /// from the published one.
    Rebuild(OracleArgs),
}

#[derive(Args)]
struct Sources {
    /// Manifests by id, `<MANIFEST ID>.manifest`. A manifest not there is downloaded into it.
    #[arg(long, env = "CENSUS_SYNC_MANIFESTS", default_value = "data/manifests")]
    manifests: Utf8PathBuf,
    /// The chunk cache: the frames downloaded from the CDN.
    #[arg(long, env = "CENSUS_SYNC_CACHE", default_value = "data/chunks")]
    cache: Utf8PathBuf,
    /// Whole bundles at their CDN paths under this directory, read before the cache and the CDN.
    #[arg(long, env = "CENSUS_SYNC_MIRROR")]
    mirror: Option<Utf8PathBuf>,
    /// Read the manifests, the mirror and the cache only; download nothing.
    #[arg(long)]
    offline: bool,
    /// One host serving manifests and bundles at Riot's paths, such as a mirror served over HTTP,
    /// in place of Riot's CDN.
    #[arg(long)]
    cdn_host: Option<String>,
}

#[derive(Args)]
struct StatusArgs {
    /// The working clone of the history, a bare repository.
    #[arg(long)]
    repo: Utf8PathBuf,
    /// The branch the builds are appended to.
    #[arg(long, default_value = HISTORY)]
    branch: String,
    /// A clone of the manifest list, checked out at the last commit whose builds were appended.
    #[arg(long, env = "CENSUS_SYNC_LIST", default_value = "data/riot-manifests")]
    list: Utf8PathBuf,
    /// Read the list as last fetched.
    #[arg(long)]
    no_fetch: bool,
    /// Manifests by id, and the days they were published.
    #[arg(long, env = "CENSUS_SYNC_MANIFESTS", default_value = "data/manifests")]
    manifests: Utf8PathBuf,
    /// One host serving manifests at Riot's paths, in place of Riot's CDN.
    #[arg(long)]
    cdn_host: Option<String>,
}

#[derive(Args)]
struct AppendArgs {
    /// The working clone of the history, a bare repository.
    #[arg(long)]
    repo: Utf8PathBuf,
    #[command(flatten)]
    sources: Sources,
    /// The branch to append to.
    #[arg(long, default_value = HISTORY)]
    branch: String,
    /// Set the branch to this commit first.
    #[arg(long)]
    start: Option<String>,
    /// A clone of the manifest list, checked out at the last commit whose builds were appended.
    #[arg(long, env = "CENSUS_SYNC_LIST", default_value = "data/riot-manifests")]
    list: Utf8PathBuf,
    /// The client's version, `16.19.8207193`; by default the name the manifest list gives the
    /// build.
    #[arg(long)]
    version: Option<String>,
    /// The day the manifest was published, `YYYY-MM-DD`; by default its `Last-Modified` on the
    /// CDN, in UTC.
    #[arg(long)]
    date: Option<String>,
    /// The build's manifest id, 16 hex.
    manifest: String,
}

#[derive(Args)]
struct CheckArgs {
    /// The working clone of the history, a bare repository.
    #[arg(long)]
    repo: Utf8PathBuf,
    #[command(flatten)]
    sources: Sources,
    /// The branch whose commits are checked.
    #[arg(long, default_value = HISTORY)]
    branch: String,
    /// The last commit already pushed: the commits after it on the branch are checked.
    since: String,
}

#[derive(Args)]
struct OracleArgs {
    /// The working clone of the history, a bare repository.
    #[arg(long)]
    repo: Utf8PathBuf,
    #[command(flatten)]
    sources: Sources,
    /// The branch the builds are re-appended on.
    #[arg(long, default_value = "oracle")]
    branch: String,
    /// The published commit the range starts after.
    commit: String,
    /// How many of the builds after it on `history-v2` to re-append.
    count: usize,
}

#[derive(Args)]
struct VerifyArgs {
    #[command(flatten)]
    sources: Sources,
    /// Every file of each manifest, not only the WADs the history reads.
    #[arg(long)]
    all_files: bool,
    /// The manifests, 16 hex each, in the order that names the first build and file using a chunk.
    #[arg(required = true)]
    manifests: Vec<String>,
}

/// The manifests and chunk sources of a run. Chunks are asked of the mirror when one is given, then
/// of the CDN through the chunk cache, or of the cache alone when offline.
struct Inputs {
    manifests: Utf8PathBuf,
    mirror: Option<BundleMirror>,
    cdn: Option<Cdn>,
    chunks: Box<dyn ChunkSource>,
}

/// Riot's CDN, or `host` in its place.
fn cdn_at(host: Option<&str>) -> Cdn {
    match host {
        Some(host) => Cdn::new().with_host(host),
        None => Cdn::new(),
    }
}

impl Inputs {
    fn open(sources: &Sources) -> Result<Self> {
        let cache = ChunkCache::open(&sources.cache).with_context(|| format!("opening the chunk cache {}", sources.cache))?;
        let cdn = (!sources.offline).then(|| cdn_at(sources.cdn_host.as_deref()));
        let chunks: Box<dyn ChunkSource> = match &cdn {
            Some(cdn) => Box::new(CdnSource::new(cdn.clone(), cache)),
            None => Box::new(cache),
        };
        let mirror = sources.mirror.as_deref().map(BundleMirror::new);
        Ok(Self { manifests: sources.manifests.clone(), mirror, cdn, chunks })
    }

    fn manifest(&self, id: u64) -> Result<Manifest> {
        Ok(match &self.cdn {
            Some(cdn) => cdn.manifest(id, &self.manifests)?,
            None => Manifest::read(&self.manifests.join(format!("{id:016X}.manifest")))?,
        })
    }

    fn source(&self) -> Layers<'_> {
        let mut layers: Vec<&dyn ChunkSource> = Vec::new();
        if let Some(mirror) = &self.mirror {
            layers.push(mirror);
        }
        layers.push(self.chunks.as_ref());
        Layers::new(layers)
    }

    /// The day a manifest was published, from the CDN.
    fn published(&self, id: u64) -> Result<String> {
        let cdn = self.cdn.as_ref().with_context(|| format!("offline, so the day manifest {id:016x} was published is not known; give --date"))?;
        Ok(cdn.published(id, &self.manifests)?)
    }

    fn downloaded(&self) -> Downloaded {
        self.cdn.as_ref().map(Cdn::downloaded).unwrap_or_default()
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        // fjall logs each flush and compaction of the chunk cache at info.
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,fjall=warn,lsm_tree=warn".into()))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .init();
    let cli = Cli::parse();
    let jobs = cli.jobs.unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()));
    let pool = rayon::ThreadPoolBuilder::new().num_threads(jobs).build()?;
    match cli.command {
        Command::Status(args) => run_status(&args),
        Command::Append(args) => run_append(&args, &pool),
        Command::Check(args) => run_check(&args, &pool),
        Command::Oracle(args) => run_oracle(&args, &pool),
        Command::Verify(args) => run_verify(&args, &pool),
        Command::Rebuild(args) => run_rebuild(&args, &pool),
    }
}

fn hex_id(text: &str) -> Result<u64> {
    u64::from_str_radix(text, 16).with_context(|| format!("{text}: a manifest id is 16 hex"))
}

fn run_status(args: &StatusArgs) -> Result<()> {
    let git = Git::open(&args.repo)?;
    let list = ManifestList::open(&args.list)?;
    if !args.no_fetch {
        list.fetch()?;
    }
    let branch = format!("refs/heads/{}", args.branch);
    let tip = build_facts(&git.run(&["cat-file", "blob", &format!("{branch}:build.yaml")])?)?;
    println!("tip\t{}\t{:016x}\t{}", tip.version, tip.manifest, tip.date);
    let (from, to) = (list.checkout()?, list.upstream()?);
    let listed = list.added(&from, &to, LIVE_REALM)?;
    let appended = manifests(&git, &branch)?;
    let cdn = cdn_at(args.cdn_host.as_deref());
    let mut new = 0;
    for build in listed.iter().filter(|b| !appended.contains(&b.manifest)) {
        let date = cdn.published(build.manifest, &args.manifests)?;
        println!("new\t{}\t{:016x}\t{date}", build.version, build.manifest);
        new += 1;
    }
    tracing::info!("the list's commits {}..{}: {} {LIVE_REALM} builds, {new} new", &from[..9], &to[..9], listed.len());
    Ok(())
}

/// A build's facts: the history's realm is `LIVE_REALM` alone.
fn facts_of(version: &str, manifest: u64, date: &str) -> Result<BuildFacts> {
    let mut parts = version.split('.').map(str::parse::<u16>);
    let (Some(Ok(season)), Some(Ok(patch))) = (parts.next(), parts.next()) else {
        bail!("{version}: a version is season.patch.build");
    };
    Ok(BuildFacts {
        version: version.to_string(),
        patch: format!("{season}.{patch}"),
        manifest,
        rads: None,
        date: date.to_string(),
        legacy_bins: legacy_bins(season, patch),
        realms: vec![LIVE_REALM.to_string()],
    })
}

fn run_append(args: &AppendArgs, pool: &rayon::ThreadPool) -> Result<()> {
    let git = Git::open(&args.repo)?;
    let branch = format!("refs/heads/{}", args.branch);
    if let Some(start) = &args.start {
        let commit = git.rev_parse(&format!("{start}^{{commit}}"))?;
        git.reset_ref(&branch, &commit)?;
    }
    let manifest = hex_id(&args.manifest)?;
    if manifests(&git, &branch)?.contains(&manifest) {
        bail!("{} already holds manifest {manifest:016x}", args.branch);
    }
    let inputs = Inputs::open(&args.sources)?;
    let version = match &args.version {
        Some(version) => version.clone(),
        None => {
            let list = ManifestList::open(&args.list)?;
            let listed = list.added(&list.checkout()?, &list.upstream()?, LIVE_REALM)?;
            listed.into_iter().find(|b| b.manifest == manifest).map(|b| b.version).with_context(|| {
                format!("manifest {manifest:016x} is not among the {LIVE_REALM} builds the manifest list added after its checkout; give --version")
            })?
        }
    };
    let date = match &args.date {
        Some(date) => date.clone(),
        None => inputs.published(manifest)?,
    };
    let facts = facts_of(&version, manifest, &date)?;
    let started = Instant::now();
    let appended = append(&git, &args.branch, &facts, &inputs.manifest(manifest)?, &inputs.source(), pool)?;
    log(&facts, &appended, started, Downloaded::default(), inputs.downloaded());
    Ok(())
}

fn run_check(args: &CheckArgs, pool: &rayon::ThreadPool) -> Result<()> {
    let git = Git::open(&args.repo)?;
    let branch = format!("refs/heads/{}", args.branch);
    let base = git.rev_parse(&format!("{}^{{commit}}", args.since))?;
    if git.run(&["merge-base", "--is-ancestor", &base, &branch]).is_err() {
        bail!("{} is not on {}", args.since, args.branch);
    }
    let range = format!("{base}..{branch}");
    let commits: Vec<String> = git.run(&["rev-list", "--reverse", "--first-parent", &range])?.lines().map(str::to_string).collect();
    let inputs = Inputs::open(&args.sources)?;
    let source = inputs.source();
    let mut failed = 0;
    for (i, commit) in commits.iter().enumerate() {
        let started = Instant::now();
        let facts = build_facts(&git.run(&["cat-file", "blob", &format!("{commit}:build.yaml")])?)?;
        let checked = check(&git, commit, &inputs.manifest(facts.manifest)?, &source, pool)?;
        for problem in &checked.problems {
            println!("{}\t{}\t{problem}", &commit[..12], facts.version);
        }
        tracing::info!(
            "{}/{} {} {}: {} files checked, {} WADs against their tables, {} problems ({:.1}s)",
            i + 1,
            commits.len(),
            facts.version,
            &commit[..12],
            checked.files,
            checked.wads,
            checked.problems.len(),
            started.elapsed().as_secs_f64()
        );
        failed += usize::from(!checked.problems.is_empty());
    }
    if failed > 0 {
        bail!("{failed} of {} commits have problems", commits.len());
    }
    tracing::info!("{} commits after {} on {}: every check passes", commits.len(), args.since, args.branch);
    Ok(())
}

/// The commit `commit` names, and the `count` builds after it on `history-v2`.
fn published_after(git: &Git, commit: &str, count: usize) -> Result<(String, Vec<String>)> {
    let base = git.rev_parse(&format!("{commit}^{{commit}}"))?;
    let range = format!("{base}..{HISTORY}");
    let published: Vec<String> =
        git.run(&["rev-list", "--reverse", "--first-parent", "--ancestry-path", &range])?.lines().take(count).map(str::to_string).collect();
    if published.len() < count {
        bail!("{HISTORY} has {} builds after {commit}, not {count}", published.len());
    }
    Ok((base, published))
}

fn run_oracle(args: &OracleArgs, pool: &rayon::ThreadPool) -> Result<()> {
    let git = Git::open(&args.repo)?;
    let (_, published) = published_after(&git, &args.commit, args.count)?;
    let inputs = Inputs::open(&args.sources)?;
    let source = inputs.source();
    let branch = format!("refs/heads/{}", args.branch);
    let started = Instant::now();
    let (mut trees, mut commits, mut failed) = (0, 0, Vec::new());
    for (i, expected) in published.iter().enumerate() {
        let (build, before) = (Instant::now(), inputs.downloaded());
        let facts = build_facts(&git.run(&["cat-file", "blob", &format!("{expected}:build.yaml")])?)?;
        // Onto the published parent, so one build that differs does not hide the ones after it.
        git.reset_ref(&branch, &git.rev_parse(&format!("{expected}^"))?)?;
        let appended = match inputs.manifest(facts.manifest).and_then(|m| Ok(append(&git, &args.branch, &facts, &m, &source, pool)?)) {
            Ok(appended) => appended,
            Err(e) => {
                tracing::error!("{}/{} {}: stopped: {e:#}", i + 1, published.len(), facts.version);
                failed.push(facts.version);
                continue;
            }
        };
        log(&facts, &appended, build, before, inputs.downloaded());
        let tree = git.rev_parse(&format!("{expected}^{{tree}}"))?;
        if appended.tree != tree {
            let diff = git.run(&["diff-tree", "-r", "--name-status", expected, &appended.commit])?;
            let lines: Vec<&str> = diff.lines().collect();
            tracing::error!("{}/{} {}: DIFF, {} paths:\n{}", i + 1, published.len(), facts.version, lines.len(), lines[..lines.len().min(20)].join("\n"));
            failed.push(facts.version);
            continue;
        }
        trees += 1;
        let same = appended.commit == *expected;
        commits += usize::from(same);
        tracing::info!("{}/{} {}: same tree, {} commit", i + 1, published.len(), facts.version, if same { "same" } else { "another" });
    }
    let seconds = started.elapsed().as_secs_f64();
    tracing::info!(
        "{trees} of {} trees identical, {commits} commits identical, {seconds:.0}s ({:.1}s a build)",
        published.len(),
        seconds / published.len() as f64
    );
    if !failed.is_empty() || commits != published.len() {
        bail!("{} builds differ or stopped: {}", published.len() - commits, failed.join(" "));
    }
    Ok(())
}

fn run_rebuild(args: &OracleArgs, pool: &rayon::ThreadPool) -> Result<()> {
    let git = Git::open(&args.repo)?;
    let (base, published) = published_after(&git, &args.commit, args.count)?;
    let inputs = Inputs::open(&args.sources)?;
    let source = inputs.source();
    git.reset_ref(&format!("refs/heads/{}", args.branch), &base)?;
    let started = Instant::now();
    let mut rebuilt = Vec::with_capacity(published.len());
    for (i, expected) in published.iter().enumerate() {
        let (build, before) = (Instant::now(), inputs.downloaded());
        let facts = build_facts(&git.run(&["cat-file", "blob", &format!("{expected}:build.yaml")])?)?;
        let appended = append(&git, &args.branch, &facts, &inputs.manifest(facts.manifest)?, &source, pool)?;
        log(&facts, &appended, build, before, inputs.downloaded());
        let differs = git.run(&["diff-tree", "-r", "--name-only", expected, &appended.commit])?.lines().count();
        match differs {
            0 if appended.commit == *expected => tracing::info!("{}/{} {}: the published commit", i + 1, published.len(), facts.version),
            0 => tracing::info!("{}/{} {}: the published tree, another commit", i + 1, published.len(), facts.version),
            n => tracing::warn!("{}/{} {}: {n} paths differ from the published tree", i + 1, published.len(), facts.version),
        }
        rebuilt.push(appended.commit);
    }
    let changed = published.iter().zip(&rebuilt).filter(|(p, r)| p != r).count();
    tracing::info!(
        "{} builds rebuilt on {}, {changed} commits new, {:.0}s",
        published.len(),
        args.branch,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn run_verify(args: &VerifyArgs, pool: &rayon::ThreadPool) -> Result<()> {
    let inputs = Inputs::open(&args.sources)?;
    let source = inputs.source();
    let mut seen = HashSet::new();
    // Each chunk with the first manifest and file that use it, the file's path interned: a path
    // repeats in every build.
    let mut chunks: Vec<(ChunkRef, u64, u32)> = Vec::new();
    let mut paths: Vec<String> = Vec::new();
    let mut path_index: HashMap<String, u32> = HashMap::new();
    for id in &args.manifests {
        let manifest = inputs.manifest(hex_id(id)?)?;
        for file in manifest.files.iter().filter(|f| args.all_files || f.path.ends_with(".wad.client")) {
            let fresh: Vec<ChunkRef> = manifest.chunks_of(file)?.into_iter().filter(|c| seen.insert(c.id)).collect();
            if fresh.is_empty() {
                continue;
            }
            let path = *path_index.entry(file.path.clone()).or_insert_with(|| {
                paths.push(file.path.clone());
                (paths.len() - 1) as u32
            });
            chunks.extend(fresh.into_iter().map(|c| (c, manifest.id, path)));
        }
    }
    let what = if args.all_files { "files" } else { "WADs" };
    tracing::info!("{} chunks in the {what} of {} manifests", chunks.len(), args.manifests.len());
    let started = Instant::now();
    let done = AtomicUsize::new(0);
    let mut bad: Vec<(ChunkRef, u64, u32, String)> = pool.install(|| {
        chunks
            .par_iter()
            .filter_map(|(chunk, manifest, path)| {
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                if n.is_multiple_of(100_000) {
                    tracing::info!("{n} of {} chunks ({:.0}s)", chunks.len(), started.elapsed().as_secs_f64());
                }
                let checked = source.chunks(std::slice::from_ref(chunk));
                checked.err().map(|e| (*chunk, *manifest, *path, e.to_string()))
            })
            .collect()
    });
    bad.sort_by_key(|(chunk, ..)| chunk.id);
    for (chunk, manifest, path, problem) in &bad {
        let p = chunk.place;
        let path = &paths[*path as usize];
        println!("{:016x}\t{:016X}\t{}\t{}\t{}\t{manifest:016X}\t{path}\t{problem}", chunk.id, p.bundle, p.offset, p.compressed_size, p.uncompressed_size);
    }
    tracing::info!("{} of {} chunks bad ({:.0}s)", bad.len(), chunks.len(), started.elapsed().as_secs_f64());
    if !bad.is_empty() {
        bail!("{} chunks do not verify", bad.len());
    }
    Ok(())
}

fn log(facts: &BuildFacts, a: &Appended, started: Instant, before: Downloaded, after: Downloaded) {
    tracing::info!(
        "{} {:016x}: {} of {} WADs changed, {} entries read, {} files written, {} removed{}{} -> commit {} tree {} ({:.1}s)",
        facts.version,
        facts.manifest,
        a.changed,
        a.wads,
        a.read,
        a.written,
        a.removed,
        if a.unrenderable + a.repeated > 0 {
            format!(" ({} bin objects unrenderable, {} under a repeated entry hash)", a.unrenderable, a.repeated)
        } else {
            String::new()
        },
        match after.requests - before.requests {
            0 => String::new(),
            n => format!(", {:.1} MB downloaded in {n} requests", (after.bytes - before.bytes) as f64 / 1e6),
        },
        &a.commit[..12],
        &a.tree[..12],
        started.elapsed().as_secs_f64()
    );
}
