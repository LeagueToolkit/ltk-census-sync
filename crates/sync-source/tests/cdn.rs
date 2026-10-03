//! The CDN source against a local HTTP server: answers recorded from Riot's CDN, replayed, and
//! answers the test builds from a bundle it writes.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use camino::Utf8PathBuf;
use common::{chunks_of, file_of};
use sync_source::{BundleChunk, Cdn, CdnSource, ChunkCache, ChunkHash, ChunkRef, ChunkSource, Error, FileReader, Layers};
use xxhash_rust::xxh64::xxh64;

/// A request the server saw: its path and its `Range` header.
type Seen = Arc<Mutex<Vec<(String, Option<String>)>>>;

/// A server answering each request with what `answer` makes of its path and `Range` header: a
/// whole HTTP response. A connection stays open for the next request unless the response closes
/// it.
fn serve(answer: impl Fn(&str, Option<&str>) -> Vec<u8> + Send + 'static) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut request = String::new();
                if reader.read_line(&mut request).unwrap_or(0) == 0 {
                    break;
                }
                let mut range = None;
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 2 {
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("range")
                    {
                        range = Some(value.trim().to_string());
                    }
                    line.clear();
                }
                let path = request.split_whitespace().nth(1).unwrap_or_default().to_string();
                let response = answer(&path, range.as_deref());
                log.lock().unwrap().push((path, range));
                stream.write_all(&response).unwrap();
                let head = String::from_utf8_lossy(&response[..response.len().min(4096)]).to_lowercase();
                if head.contains("\r\nconnection: close\r\n") {
                    break;
                }
            }
        }
    });
    (host, seen)
}

fn fixture(name: &str) -> Vec<u8> {
    fs_err::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// The CDN at `host` as a chunk source, read through `cache`.
fn source(host: &str, cache: &ChunkCache) -> CdnSource {
    CdnSource::new(Cdn::new().with_host(host), cache.clone())
}

fn cache() -> (tempfile::TempDir, ChunkCache) {
    let dir = tempfile::tempdir().unwrap();
    let cache = ChunkCache::open(&Utf8PathBuf::from_path_buf(dir.path().join("chunks")).unwrap()).unwrap();
    (dir, cache)
}

/// The recorded bundle, from the 16.19.8207193 manifest: three chunks of `DATA/FINAL/UI.wad.client`,
/// the first two end to end and the third after a chunk of 667 bytes.
const RECORDED: u64 = 0x8250_C7AC_1283_3936;

fn recorded(id: u64, offset: u64, compressed_size: u32) -> ChunkRef {
    let place = BundleChunk { bundle: RECORDED, offset, compressed_size, uncompressed_size: compressed_size - 10 };
    ChunkRef { id, hash: ChunkHash::Blake3, place }
}

fn recorded_chunks() -> [ChunkRef; 3] {
    [
        recorded(0x3529_b475_3a79_e823, 3_643_797, 876),
        recorded(0x73d0_5354_b986_36f1, 3_644_673, 894),
        recorded(0x3a0c_05ae_b86d_d31b, 3_646_234, 2428),
    ]
}

fn sizes(data: &[Vec<u8>]) -> Vec<usize> {
    data.iter().map(Vec::len).collect()
}

#[test]
fn a_recorded_multipart_answer_gives_each_chunk_checked_and_fills_the_cache() {
    let answer = fixture("two-spans.http");
    let (host, seen) = serve(move |_, _| answer.clone());
    let (_dir, cache) = cache();
    let cdn = source(&host, &cache);
    let chunks = recorded_chunks();
    assert_eq!(sizes(&cdn.chunks(&chunks).unwrap()), [866, 884, 2418]);
    assert_eq!(
        *seen.lock().unwrap(),
        [("/channels/public/bundles/8250C7AC12833936.bundle".to_string(), Some("bytes=3643797-3645566,3646234-3648661".to_string()))]
    );
    // From the cache now, with no request.
    assert_eq!(sizes(&cdn.chunks(&[chunks[2], chunks[0], chunks[2]]).unwrap()), [2418, 866, 2418]);
    assert_eq!(sizes(&cache.chunks(&chunks).unwrap()), [866, 884, 2418]);
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn a_recorded_single_range_answer_gives_its_chunk() {
    let answer = fixture("one-span.http");
    let (host, seen) = serve(move |_, _| answer.clone());
    let (_dir, cache) = cache();
    let cdn = source(&host, &cache);
    let chunk = recorded_chunks()[2];
    assert_eq!(sizes(&cdn.chunks(&[chunk, chunk]).unwrap()), [2418, 2418]);
    assert_eq!(seen.lock().unwrap()[0].1.as_deref(), Some("bytes=3646234-3648661"));
}

/// Frames laid out in one bundle with `gap` bytes before each: the chunks placed where it holds
/// them, and its bytes.
fn bundle(id: u64, gap: usize, chunks: &mut [(ChunkRef, Vec<u8>)]) -> Vec<u8> {
    let mut data = Vec::new();
    for (chunk, frame) in chunks.iter_mut() {
        data.extend(std::iter::repeat_n(0xEE, gap));
        chunk.place.bundle = id;
        chunk.place.offset = data.len() as u64;
        chunk.place.compressed_size = frame.len() as u32;
        data.extend(&*frame);
    }
    data
}

/// The answer the CDN gives to a `Range` header over `data`, in the shape recorded from it.
fn multipart(data: &[u8], range: &str) -> Vec<u8> {
    let mut body: Vec<u8> = Vec::new();
    for span in range.strip_prefix("bytes=").unwrap().split(',') {
        let (first, last) = span.split_once('-').unwrap();
        let (first, last): (usize, usize) = (first.parse().unwrap(), last.parse().unwrap());
        let head = format!("\r\n--7A530064C55C24FE\r\nContent-Type: binary/octet-stream\r\nContent-Range: bytes {first}-{last}/{}\r\n\r\n", data.len());
        body.extend(head.as_bytes());
        body.extend(&data[first..=last]);
    }
    body.extend(b"\r\n--7A530064C55C24FE--\r\n");
    let mut response = b"HTTP/1.1 206 Partial Content\r\nContent-Type: multipart/byteranges; boundary=7A530064C55C24FE\r\nConnection: close\r\n\r\n".to_vec();
    response.extend(body);
    response
}

fn text(len: usize) -> Vec<u8> {
    (0..len).map(|i| b"abcdefghijklmnopqrstuvwxyz"[i * 7 % 26]).collect()
}

#[test]
fn spans_past_128_go_in_further_requests() {
    let bytes = text(3000);
    let mut chunks = chunks_of(&bytes, 10);
    let data = bundle(0xB1, 1, &mut chunks);
    let (host, seen) = serve(move |_, range| multipart(&data, range.unwrap()));
    let (_dir, cache) = cache();
    let cdn = source(&host, &cache);
    let refs: Vec<ChunkRef> = chunks.iter().map(|(c, _)| *c).collect();
    assert_eq!(cdn.chunks(&refs).unwrap().concat(), bytes);
    let spans: Vec<usize> = seen.lock().unwrap().iter().map(|(_, r)| r.as_ref().unwrap().split(',').count()).collect();
    assert_eq!(spans, [128, 128, 44]);
}

#[test]
fn chunks_end_to_end_go_in_one_span_and_a_whole_bundle_answer_is_read_by_their_places() {
    let bytes = text(300);
    let mut chunks = chunks_of(&bytes, 100);
    let data = bundle(0xB1, 0, &mut chunks);
    let (host, seen) = serve(move |_, _| {
        let mut response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", data.len()).into_bytes();
        response.extend(&data);
        response
    });
    let (_dir, cache) = cache();
    let cdn = source(&host, &cache);
    let refs: Vec<ChunkRef> = chunks.iter().map(|(c, _)| *c).collect();
    assert_eq!(cdn.chunks(&refs).unwrap().concat(), bytes);
    let last = chunks[2].0.place;
    assert_eq!(seen.lock().unwrap()[0].1, Some(format!("bytes=0-{}", last.offset + u64::from(last.compressed_size) - 1)));
}

#[test]
fn a_chunk_a_source_or_the_cache_holds_wrong_comes_from_the_cdn_and_is_kept() {
    let bytes = text(300);
    let mut chunks = chunks_of(&bytes, 100);
    let data = bundle(0xB1, 3, &mut chunks);
    let (host, seen) = serve(move |_, range| multipart(&data, range.unwrap()));
    let (_dir, cache) = cache();
    let refs: Vec<ChunkRef> = chunks.iter().map(|(c, _)| *c).collect();
    // The memory source holds the first chunk's frame for the second and lacks the third; the
    // cache holds the first's frame for the third.
    let (mut memory, _) = file_of(&bytes, 100);
    memory.frames.insert(refs[1].id, chunks[0].1.clone());
    memory.frames.remove(&refs[2].id);
    cache.put(&refs[2], &chunks[0].1).unwrap();
    let cdn = source(&host, &cache);
    let layers = Layers::new(vec![&memory, &cdn]);
    let mut file = FileReader::new(&layers, refs.clone());
    assert_eq!(file.read_range(0, 300).unwrap(), bytes);
    let ranges: Vec<String> = seen.lock().unwrap().iter().map(|(_, r)| r.clone().unwrap()).collect();
    let span = |c: &ChunkRef| format!("{}-{}", c.place.offset, c.place.offset + u64::from(c.place.compressed_size) - 1);
    assert_eq!(ranges, [format!("bytes={}", span(&refs[1])), format!("bytes={}", span(&refs[2]))]);
    assert_eq!(cache.frame(&refs[2]).unwrap(), Some(chunks[2].1.clone()));
    assert_eq!(cache.chunks(&refs[1..]).unwrap().concat(), &bytes[100..]);
}

#[test]
fn an_answer_that_lacks_a_chunk_or_does_not_check_or_is_not_found_is_refused() {
    let bytes = text(200);
    let mut chunks = chunks_of(&bytes, 100);
    let data = bundle(0xB1, 0, &mut chunks);
    let first = chunks[0].0.place;
    let (host, _) = serve(move |path, _| {
        if path.contains("00000000000000B2") {
            return b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec();
        }
        // Always the first chunk's frame, whatever was asked for.
        let end = first.compressed_size as usize - 1;
        let mut response =
            format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-{end}/{}\r\nContent-Length: {}\r\n\r\n", data.len(), end + 1).into_bytes();
        response.extend(&data[..=end]);
        response
    });
    let (_dir, cache) = cache();
    let cdn = source(&host, &cache);
    let refs: Vec<ChunkRef> = chunks.iter().map(|(c, _)| *c).collect();
    assert!(matches!(cdn.chunks(&refs), Err(Error::Download { message, .. }) if message.contains("no part of the answer holds chunk")));
    // The first chunk's frame placed where the second is: it does not check, and is not kept.
    let mut moved = refs[1];
    moved.place.offset = 0;
    moved.place.compressed_size = first.compressed_size;
    assert!(matches!(cdn.chunks(&[moved]), Err(Error::BadChunk { id, .. }) if id == refs[1].id));
    assert_eq!(cache.frame(&moved).unwrap(), None);
    // The first chunk was kept by the first call, wherever a manifest places it; the second was not.
    let mut elsewhere = refs[0];
    elsewhere.place.bundle = 0xB2;
    assert_eq!(cdn.chunks(&[elsewhere]).unwrap().concat(), &bytes[..100]);
    elsewhere.id = refs[1].id;
    assert!(matches!(cdn.chunks(&[elsewhere]), Err(Error::Download { message, .. }) if message == "status 404"));
}

#[test]
fn the_chunks_an_answer_leaves_out_are_asked_for_again() {
    let bytes = text(500);
    let mut chunks = chunks_of(&bytes, 100);
    let data = bundle(0xB1, 2, &mut chunks);
    let answers = Arc::new(Mutex::new(0));
    let count = answers.clone();
    // The first answer leaves out the last part asked for; the ones after are whole.
    let (host, seen) = serve(move |_, range| {
        let mut count = count.lock().unwrap();
        *count += 1;
        let range = range.unwrap();
        match range.rsplit_once(',') {
            Some((all_but_last, _)) if *count == 1 => multipart(&data, all_but_last),
            _ => multipart(&data, range),
        }
    });
    let (_dir, cache) = cache();
    let cdn = source(&host, &cache);
    let refs: Vec<ChunkRef> = chunks.iter().map(|(c, _)| *c).collect();
    assert_eq!(cdn.chunks(&refs).unwrap().concat(), bytes);
    let last = refs[4].place;
    let ranges: Vec<String> = seen.lock().unwrap().iter().map(|(_, r)| r.clone().unwrap()).collect();
    assert_eq!(ranges[1], format!("bytes={}-{}", last.offset, last.offset + u64::from(last.compressed_size) - 1));
    assert_eq!(ranges.len(), 2);
}

#[test]
fn an_answer_that_always_leaves_a_chunk_out_is_refused_after_three() {
    let bytes = text(200);
    let mut chunks = chunks_of(&bytes, 100);
    let data = bundle(0xB1, 2, &mut chunks);
    let first = chunks[0].0.place;
    let only_first = format!("bytes={}-{}", first.offset, first.offset + u64::from(first.compressed_size) - 1);
    let (host, seen) = serve(move |_, _| multipart(&data, &only_first));
    let (_dir, cache) = cache();
    let cdn = source(&host, &cache);
    let refs: Vec<ChunkRef> = chunks.iter().map(|(c, _)| *c).collect();
    let error = cdn.chunks(&refs).unwrap_err();
    assert!(matches!(&error, Error::Download { message, .. } if message.contains(&format!("chunk {:016x}", refs[1].id)) && message.contains("1 parts")), "{error}");
    assert_eq!(seen.lock().unwrap().len(), 3);
}

/// A manifest with no files: the header and a zstd body of an empty flatbuffer table.
fn empty_manifest() -> (u64, Vec<u8>) {
    let mut fb = flatbuffers::FlatBufferBuilder::new();
    let start = fb.start_table();
    let root = fb.end_table(start);
    fb.finish_minimal(root);
    let body = fb.finished_data();
    let id = xxh64(body, 0);
    let compressed = zstd::bulk::compress(body, 3).unwrap();
    let mut bytes = b"RMAN\x02\x00\x00\x00".to_vec();
    bytes.extend(28u32.to_le_bytes());
    bytes.extend((compressed.len() as u32).to_le_bytes());
    bytes.extend(id.to_le_bytes());
    bytes.extend((body.len() as u32).to_le_bytes());
    bytes.extend(compressed);
    (id, bytes)
}

#[test]
fn a_manifest_is_downloaded_once_and_checked_against_its_id() {
    let (id, manifest) = empty_manifest();
    let (host, seen) = serve(move |_, _| {
        let mut response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", manifest.len()).into_bytes();
        response.extend(&manifest);
        response
    });
    let dir = tempfile::tempdir().unwrap();
    let manifests = Utf8PathBuf::from_path_buf(dir.path().join("manifests")).unwrap();
    let cdn = Cdn::new().with_host(&host);
    assert_eq!(cdn.manifest(id, &manifests).unwrap().id, id);
    assert_eq!(seen.lock().unwrap()[0].0, format!("/channels/public/releases/{id:016X}.manifest"));
    assert!(manifests.join(format!("{id:016X}.manifest")).is_file());
    assert_eq!(cdn.manifest(id, &manifests).unwrap().id, id);
    assert_eq!(seen.lock().unwrap().len(), 1);
    // Another id answered with this manifest.
    let other = id ^ 1;
    assert!(matches!(cdn.manifest(other, &manifests), Err(Error::Download { message, .. }) if message.contains("the manifest's id is")));
    assert!(!manifests.join(format!("{other:016X}.manifest")).exists());
}

#[test]
fn the_day_a_manifest_was_published_is_its_last_modified_in_utc_asked_once() {
    let (host, seen) = serve(|_, _| b"HTTP/1.1 200 OK\r\nLast-Modified: Wed, 13 May 2026 23:33:33 GMT\r\nContent-Length: 0\r\n\r\n".to_vec());
    let dir = tempfile::tempdir().unwrap();
    let manifests = Utf8PathBuf::from_path_buf(dir.path().join("manifests")).unwrap();
    let cdn = Cdn::new().with_host(&host);
    assert_eq!(cdn.published(0xAB, &manifests).unwrap(), "2026-05-13");
    assert_eq!(fs_err::read_to_string(manifests.join("00000000000000AB.date")).unwrap(), "2026-05-13\n");
    assert_eq!(cdn.published(0xAB, &manifests).unwrap(), "2026-05-13");
    assert_eq!(*seen.lock().unwrap(), [("/channels/public/releases/00000000000000AB.manifest".to_string(), None)]);
    assert_eq!(cdn.downloaded().requests, 1);
}
