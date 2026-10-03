//! The CDN source against a local HTTP server serving a bundle the test writes.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use camino::Utf8PathBuf;
use common::{chunks_of, file_of};
use sync_source::{BundleMirror, Cdn, ChunkRef, ChunkSource, Error, FileReader, Layers};

/// A bundle of these frames as Riot's are laid out: the frames, the table, and a footer naming
/// `named`. Each chunk's place is set to where the bundle holds it.
fn bundle(id: u64, named: u64, chunks: &mut [(ChunkRef, Vec<u8>)]) -> Vec<u8> {
    let (mut data, mut toc) = (Vec::new(), Vec::new());
    for (chunk, frame) in chunks.iter_mut() {
        chunk.place.bundle = id;
        chunk.place.offset = data.len() as u64;
        chunk.place.compressed_size = frame.len() as u32;
        toc.extend(chunk.id.to_le_bytes());
        toc.extend(chunk.place.uncompressed_size.to_le_bytes());
        toc.extend((frame.len() as u32).to_le_bytes());
        data.extend(&*frame);
    }
    data.extend(&toc);
    data.extend(named.to_le_bytes());
    data.extend((chunks.len() as u32).to_le_bytes());
    data.extend(1u32.to_le_bytes());
    data.extend(b"RBUN");
    data
}

/// A server answering `GET /channels/public/bundles/<ID>.bundle` from `bundles`, 404 otherwise,
/// counting the requests.
fn serve(bundles: Vec<(u64, Vec<u8>)>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(AtomicUsize::new(0));
    let counted = requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            counted.fetch_add(1, Ordering::SeqCst);
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 2 {
                line.clear();
            }
            let path = request.split_whitespace().nth(1).unwrap_or_default().to_string();
            let body = bundles.iter().find(|(id, _)| path == format!("/channels/public/bundles/{id:016X}.bundle")).map(|(_, b)| b);
            match body {
                Some(body) => {
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                    stream.write_all(body).unwrap();
                }
                None => write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap(),
            }
        }
    });
    (host, requests)
}

fn text(len: usize) -> Vec<u8> {
    (0..len).map(|i| b"abcdefghijklmnopqrstuvwxyz"[i * 7 % 26]).collect()
}

#[test]
fn a_chunk_a_source_has_wrong_comes_from_its_bundle_on_the_cdn_fetched_once() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let bytes = text(300);
    let mut chunks = chunks_of(&bytes, 100);
    let whole = bundle(0xB1, 0xB1, &mut chunks);
    let (host, requests) = serve(vec![(0xB1, whole)]);

    // The first source holds every chunk, the second with another chunk's frame.
    let (mut archive, _) = file_of(&bytes, 100);
    archive.frames.insert(chunks[1].0.id, chunks[0].1.clone());
    let cdn = Cdn::new(&host, &root);
    let mirror = BundleMirror::new(&root);
    let layers = Layers::new(vec![&mirror, &archive, &cdn]);
    let refs: Vec<ChunkRef> = chunks.iter().map(|(c, _)| *c).collect();
    let mut file = FileReader::new(&layers, refs.clone());
    assert_eq!(file.read_range(0, 300).unwrap(), bytes);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    assert!(mirror.bundle_path(0xB1).is_file());
    // Every chunk of the bundle now comes from the mirror, with no other request.
    assert_eq!(cdn.chunks(&refs).unwrap().concat(), bytes);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[test]
fn a_bundle_the_cdn_lacks_or_that_names_another_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let mut chunks = chunks_of(&text(100), 100);
    let wrong = bundle(0xB2, 0xFF, &mut chunks);
    let (host, _) = serve(vec![(0xB2, wrong)]);
    let cdn = Cdn::new(&host, &root);
    assert!(matches!(cdn.fetch(0xB2), Err(Error::Download { message, .. }) if message.contains("names bundle 00000000000000FF")));
    assert!(matches!(cdn.fetch(0xB3), Err(Error::Download { message, .. }) if message.contains("404")));
    assert!(!BundleMirror::new(&root).bundle_path(0xB2).is_file());
}
