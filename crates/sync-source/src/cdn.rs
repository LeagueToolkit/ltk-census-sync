//! Riot's CDN as a chunk source (`docs/SOURCES.md`, "Bundles and the CDN"). A read's chunks are
//! fetched by multi-range requests to their bundles, the spans of one bundle in one request, and
//! read through the chunk cache, so a chunk is downloaded once. Manifests come from the CDN too.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use camino::Utf8Path;

use crate::byteranges::Ranges;
use crate::cache::ChunkCache;
use crate::chunk::{open, ChunkSource};
use crate::mirror::bundle_url_path;
use crate::rman::{ChunkRef, Manifest};
use crate::Error;

/// Riot's bundle host, over https.
pub const BUNDLE_HOST: &str = "https://lol.dyn.riotcdn.net";
/// Riot's manifest host, over https.
pub const MANIFEST_HOST: &str = "https://lol.secure.dyn.riotcdn.net";

/// Spans one request carries at most. Past about 258 the CDN has answered with the whole bundle.
const MAX_SPANS: usize = 128;
/// Tries of a request that fails on the way or with a server error.
const ATTEMPTS: u32 = 3;
/// The largest body read: a whole bundle is tens of megabytes, a manifest about twenty.
const MAX_BODY: u64 = 1 << 30;

/// The CDN, read through a chunk cache.
pub struct Cdn {
    bundle_host: String,
    manifest_host: String,
    cache: ChunkCache,
    agent: ureq::Agent,
    requests: AtomicU64,
    bytes: AtomicU64,
}

/// What a CDN source has downloaded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Downloaded {
    pub requests: u64,
    /// Body bytes.
    pub bytes: u64,
}

/// A response, read whole.
struct Answer {
    status: u16,
    content_type: Option<String>,
    content_range: Option<String>,
    body: Vec<u8>,
}

impl Cdn {
    /// Riot's CDN, read through `cache`.
    pub fn new(cache: ChunkCache) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(300)))
            .build()
            .into();
        Self {
            bundle_host: BUNDLE_HOST.to_string(),
            manifest_host: MANIFEST_HOST.to_string(),
            cache,
            agent,
            requests: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    /// The same source at one host that serves both manifests and bundles at Riot's paths, such
    /// as a mirror served over HTTP.
    pub fn with_host(mut self, host: &str) -> Self {
        self.bundle_host = host.trim_end_matches('/').to_string();
        self.manifest_host = self.bundle_host.clone();
        self
    }

    /// What this source has downloaded so far.
    pub fn downloaded(&self) -> Downloaded {
        Downloaded { requests: self.requests.load(Ordering::Relaxed), bytes: self.bytes.load(Ordering::Relaxed) }
    }

    /// A manifest by id: from `dir/<ID>.manifest` when it is there, else downloaded, checked to be
    /// the manifest `id` names, and written there.
    pub fn manifest(&self, id: u64, dir: &Utf8Path) -> Result<Manifest, Error> {
        let path = dir.join(format!("{id:016X}.manifest"));
        if path.is_file() {
            return Manifest::read(&path);
        }
        let url = format!("{}/channels/public/releases/{id:016X}.manifest", self.manifest_host);
        let fail = |message: String| Error::Download { url: url.clone(), message };
        let answer = self.get(&url, None)?;
        if answer.status != 200 {
            return Err(fail(format!("status {}", answer.status)));
        }
        let manifest = Manifest::parse(&answer.body).map_err(|e| fail(e.to_string()))?;
        if manifest.id != id {
            return Err(fail(format!("the manifest's id is {:016X}", manifest.id)));
        }
        fs_err::create_dir_all(dir)?;
        let part = path.with_extension("manifest.part");
        fs_err::write(&part, &answer.body)?;
        fs_err::rename(&part, &path)?;
        Ok(manifest)
    }

    /// The chunks, each from the bundle and place its manifest gives: the spans of one bundle in
    /// as few requests as `MAX_SPANS` allows. Each frame that checks is kept in the cache.
    fn fetch(&self, missing: &[ChunkRef]) -> Result<HashMap<ChunkRef, Vec<u8>>, Error> {
        let mut by_bundle: BTreeMap<u64, Vec<ChunkRef>> = BTreeMap::new();
        for chunk in missing {
            by_bundle.entry(chunk.place.bundle).or_default().push(*chunk);
        }
        let mut fetched = HashMap::with_capacity(missing.len());
        for (bundle, mut chunks) in by_bundle {
            chunks.sort_by_key(|c| c.place.offset);
            chunks.dedup();
            let url = format!("{}/{}", self.bundle_host, bundle_url_path(bundle));
            // An answer has been seen to leave out a part it was asked for (`docs/RUNS.md`): the
            // chunks it lacks are asked for again.
            for attempt in 1..=ATTEMPTS {
                let mut lacking = Vec::new();
                let mut shape = String::new();
                for batch in spans(&chunks).chunks(MAX_SPANS) {
                    let ranges = self.ranges(&url, batch)?;
                    let before = lacking.len();
                    for chunk in &chunks[batch[0].2.start..batch[batch.len() - 1].2.end] {
                        let (offset, size) = (chunk.place.offset, u64::from(chunk.place.compressed_size));
                        let Some(frame) = ranges.get(offset, size) else {
                            lacking.push(*chunk);
                            continue;
                        };
                        let data = open(chunk, frame)?;
                        self.cache.put(chunk, frame)?;
                        fetched.insert(*chunk, data);
                    }
                    if lacking.len() > before {
                        shape = format!("{} spans asked, {}", batch.len(), ranges.shape());
                    }
                }
                let Some(first) = lacking.first() else { break };
                let (offset, size) = (first.place.offset, first.place.compressed_size);
                let message = format!("no part of the answer holds chunk {:016x} at {offset}+{size}; {shape}", first.id);
                if attempt == ATTEMPTS {
                    return Err(Error::Download { url, message });
                }
                tracing::warn!("{url}: {message}; asking again for the {} chunks it lacks", lacking.len());
                chunks = lacking;
            }
        }
        Ok(fetched)
    }

    /// One request for these spans of the bundle at `url`.
    fn ranges(&self, url: &str, spans: &[(u64, u64, Range<usize>)]) -> Result<Ranges, Error> {
        let list: Vec<String> = spans.iter().map(|(start, end, _)| format!("{start}-{}", end - 1)).collect();
        let answer = self.get(url, Some(&format!("bytes={}", list.join(","))))?;
        let fail = |message: String| Error::Download { url: url.to_string(), message };
        match answer.status {
            200 => Ok(Ranges::whole(answer.body)),
            206 => match answer.content_type.as_deref().and_then(boundary) {
                Some(boundary) => Ranges::multipart(answer.body, boundary),
                None => Ranges::single(answer.body, answer.content_range.as_deref().unwrap_or_default()),
            }
            .map_err(fail),
            status => Err(fail(format!("status {status}"))),
        }
    }

    /// A GET, tried again after a failure on the way or a server error.
    fn get(&self, url: &str, range: Option<&str>) -> Result<Answer, Error> {
        let mut attempt = 1;
        loop {
            match self.try_get(url, range) {
                Ok(answer) => return Ok(answer),
                Err(message) if attempt < ATTEMPTS => {
                    tracing::warn!("{url}: {message}; try {} of {ATTEMPTS}", attempt + 1);
                    std::thread::sleep(Duration::from_secs(2u64.pow(attempt)));
                    attempt += 1;
                }
                Err(message) => return Err(Error::Download { url: url.to_string(), message }),
            }
        }
    }

    fn try_get(&self, url: &str, range: Option<&str>) -> Result<Answer, String> {
        let mut request = self.agent.get(url);
        if let Some(range) = range {
            request = request.header("Range", range);
        }
        self.requests.fetch_add(1, Ordering::Relaxed);
        let mut response = request.call().map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        if status >= 500 {
            return Err(format!("status {status}"));
        }
        let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
        let (content_type, content_range) = (header("content-type"), header("content-range"));
        let body = response.body_mut().with_config().limit(MAX_BODY).read_to_vec().map_err(|e| e.to_string())?;
        self.bytes.fetch_add(body.len() as u64, Ordering::Relaxed);
        Ok(Answer { status, content_type, content_range, body })
    }
}

/// Runs of chunks that lie end to end in their bundle, in bundle order: each run's `[start, end)`
/// and its chunks' indices.
fn spans(chunks: &[ChunkRef]) -> Vec<(u64, u64, Range<usize>)> {
    let mut spans: Vec<(u64, u64, Range<usize>)> = Vec::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let (start, end) = (chunk.place.offset, chunk.place.offset + u64::from(chunk.place.compressed_size));
        match spans.last_mut() {
            Some(last) if last.1 == start => {
                last.1 = end;
                last.2.end = i + 1;
            }
            _ => spans.push((start, end, i..i + 1)),
        }
    }
    spans
}

/// The boundary of a `multipart/byteranges` content type.
fn boundary(content_type: &str) -> Option<&str> {
    let mut params = content_type.split(';');
    if !params.next()?.trim().eq_ignore_ascii_case("multipart/byteranges") {
        return None;
    }
    params.find_map(|p| p.trim().strip_prefix("boundary=")).map(|b| b.trim_matches('"'))
}

impl ChunkSource for Cdn {
    fn chunks(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, Error> {
        let mut cached = Vec::with_capacity(wanted.len());
        let mut missing = Vec::new();
        for chunk in wanted {
            match self.cache.chunks(std::slice::from_ref(chunk)) {
                Ok(mut data) => cached.push(Some(data.remove(0))),
                // A frame the cache holds that does not check is fetched again and replaced.
                Err(Error::MissingChunk(_) | Error::BadChunk { .. }) => {
                    cached.push(None);
                    missing.push(*chunk);
                }
                Err(e) => return Err(e),
            }
        }
        let mut fetched = self.fetch(&missing)?;
        let mut out: Vec<Vec<u8>> = Vec::with_capacity(wanted.len());
        for (i, (chunk, data)) in wanted.iter().zip(cached).enumerate() {
            let data = match data.or_else(|| fetched.remove(chunk)) {
                Some(data) => data,
                // A chunk asked for twice is fetched once.
                None => out[wanted[..i].iter().position(|c| c == chunk).expect("fetched for its first place")].clone(),
            };
            out.push(data);
        }
        Ok(out)
    }
}
