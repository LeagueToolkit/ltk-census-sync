mod common;

use std::io::{Read, Seek, SeekFrom};

use camino::{Utf8Path, Utf8PathBuf};
use common::{chunks_of, file_of};
use sync_source::{open_frame, BundleMirror, ChunkHash, ChunkRef, ChunkSource, Error, FileReader, Layers, MergedBundle};
use xxhash_rust::xxh64::xxh64;

fn text(len: usize) -> Vec<u8> {
    (0..len).map(|i| b"abcdefghijklmnopqrstuvwxyz"[i * 7 % 26]).collect()
}

#[test]
fn a_file_reads_across_its_chunks_and_seeks_anywhere() {
    let bytes = text(1000);
    let (source, chunks) = file_of(&bytes, 64);
    let mut file = FileReader::new(&source, chunks);
    assert_eq!(file.size(), 1000);
    assert_eq!(file.read_range(60, 200).unwrap(), &bytes[60..260]);
    assert_eq!(file.read_range(0, 1000).unwrap(), bytes);
    assert_eq!(file.read_range(999, 1).unwrap(), &bytes[999..]);
    assert!(file.read_range(0, 0).unwrap().is_empty());
    assert!(matches!(file.read_range(990, 11), Err(Error::Range { offset: 990, len: 11, size: 1000 })));

    file.seek(SeekFrom::End(-10)).unwrap();
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).unwrap();
    assert_eq!(tail, &bytes[990..]);
    file.seek(SeekFrom::Start(100)).unwrap();
    let mut some = [0u8; 30];
    file.read_exact(&mut some).unwrap();
    assert_eq!(some, bytes[100..130]);
    assert!(file.seek(SeekFrom::Current(-200)).is_err());
}

#[test]
fn a_preload_fetches_its_ranges_chunks_in_one_request_and_reads_inside_them_fetch_none() {
    let bytes = text(1000);
    let (source, chunks) = file_of(&bytes, 100);
    let mut file = FileReader::new(&source, chunks);
    file.preload(&[(150, 100), (720, 10), (300, 0)]).unwrap();
    assert_eq!(*source.asked.lock().unwrap(), [3]);
    assert_eq!(file.read_range(150, 100).unwrap(), &bytes[150..250]);
    assert_eq!(file.read_range(700, 100).unwrap(), &bytes[700..800]);
    assert_eq!(*source.asked.lock().unwrap(), [3]);
    // A read past them fetches the chunk it lacks, and holds its own chunks only.
    assert_eq!(file.read_range(250, 100).unwrap(), &bytes[250..350]);
    assert_eq!(file.read_range(710, 1).unwrap(), &bytes[710..711]);
    assert_eq!(*source.asked.lock().unwrap(), [3, 1, 1]);
    assert!(matches!(file.preload(&[(990, 11)]), Err(Error::Range { offset: 990, len: 11, size: 1000 })));
}

#[test]
fn a_chunk_that_does_not_hash_to_its_id_is_refused() {
    let bytes = text(200);
    let (mut source, chunks) = file_of(&bytes, 100);
    let other = chunks_of(&[b'z'; 100], 100).remove(0).1;
    source.frames.insert(chunks[1].id, other);
    let mut file = FileReader::new(&source, chunks.clone());
    assert_eq!(file.read_range(0, 100).unwrap(), &bytes[..100]);
    assert!(matches!(file.read_range(50, 100), Err(Error::BadChunk { id, .. }) if id == chunks[1].id));

    source.frames.remove(&chunks[0].id);
    let mut file = FileReader::new(&source, chunks.clone());
    assert!(matches!(file.read_range(0, 1), Err(Error::MissingChunk(id)) if id == chunks[0].id));
}

#[test]
fn a_chunk_is_checked_only_under_the_hash_its_file_lists() {
    let data = text(100);
    let frame = zstd::bulk::compress(&data, 1).unwrap();
    let (mut chunk, _) = chunks_of(&data, 100).remove(0);
    let mut dec = zstd::bulk::Decompressor::new().unwrap();
    assert_eq!(open_frame(&chunk, &frame, &mut dec).unwrap(), data);
    // Its BLAKE3 id, listed as SHA-256: an id of these bytes, but not under the listed hash.
    chunk.hash = ChunkHash::Sha256;
    assert!(matches!(open_frame(&chunk, &frame, &mut dec), Err(Error::BadChunk { message, .. }) if message.contains("Sha256")));
    chunk.id = ChunkHash::Sha256.id_of(&data);
    assert_eq!(open_frame(&chunk, &frame, &mut dec).unwrap(), data);
}

#[test]
fn a_merged_bundle_reads_each_copy_of_an_id_of_the_right_size_until_one_checks() {
    let dir = tempfile::tempdir().unwrap();
    let dir = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let bytes = text(300);
    let chunks = chunks_of(&bytes, 100);
    let [(a, fa), (b, fb), (c, fc)] = [chunks[0].clone(), chunks[1].clone(), chunks[2].clone()];
    let size = |r: &sync_source::ChunkRef| r.place.uncompressed_size;
    // The first part holds b's frame under a's id, at a's size; the second holds a under its id at
    // another size, then a's own frame.
    let short = zstd::bulk::compress(b"not a", 1).unwrap();
    write_part(&dir.join("t.bundle"), &[(a.id, size(&a), &fb), (b.id, size(&b), &fb)]);
    write_part(&dir.join("t.00001.bundle"), &[(c.id, size(&c), &fc), (a.id, 5, &short), (a.id, size(&a), &fa)]);

    let bundle = MergedBundle::open(&dir.join("t.bundle")).unwrap();
    assert_eq!(bundle.len(), 5);
    assert_eq!(bundle.chunks(&[c, a]).unwrap(), [&bytes[200..], &bytes[..100]]);
    let mut file = FileReader::new(&bundle, vec![a, b, c]);
    assert_eq!(file.read_range(0, 300).unwrap(), bytes);

    let mut missing = a;
    missing.id ^= 1;
    assert!(matches!(bundle.chunks(&[missing]), Err(Error::MissingChunk(_))));
    // A size no copy has is a chunk the bundle lacks; copies that do not check are a bad chunk.
    let mut other = b;
    other.place.uncompressed_size += 1;
    assert!(matches!(bundle.chunks(&[other]), Err(Error::MissingChunk(_))));
    write_part(&dir.join("t.00001.bundle"), &[(c.id, size(&c), &fc)]);
    let bundle = MergedBundle::open(&dir.join("t.bundle")).unwrap();
    assert!(matches!(bundle.chunks(&[a]), Err(Error::BadChunk { .. })));
}

#[test]
fn a_part_whose_table_does_not_match_its_footer_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("bad.bundle")).unwrap();
    let (chunk, frame) = chunks_of(b"x", 1).remove(0);
    write_part(&path, &[(chunk.id, 1, &frame)]);
    let mut bytes = fs_err::read(&path).unwrap();
    let at = bytes.len() - 20;
    bytes[at] ^= 1;
    fs_err::write(&path, bytes).unwrap();
    assert!(matches!(MergedBundle::open(&path), Err(Error::Bundle { message, .. }) if message.contains("hashes to")));
    assert!(MergedBundle::open(&path.with_file_name("none.bundle")).is_err());
}

/// A merged bundle part holding these frames, in this order.
fn write_part(path: &Utf8Path, frames: &[(u64, u32, &[u8])]) {
    let (mut data, mut toc) = (Vec::new(), Vec::new());
    for (id, uncompressed, frame) in frames {
        toc.extend(id.to_le_bytes());
        toc.extend(uncompressed.to_le_bytes());
        toc.extend((frame.len() as u32).to_le_bytes());
        data.extend(*frame);
    }
    data.extend(&toc);
    data.extend(xxh64(&toc, 0).to_le_bytes());
    data.extend((frames.len() as u32).to_le_bytes());
    data.extend(0xFFFF_FFFFu32.to_le_bytes());
    data.extend(b"RBUN");
    fs_err::write(path, data).unwrap();
}

#[test]
fn a_mirror_reads_by_the_manifests_places_and_layers_fall_through() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let bytes = text(300);
    let mut chunks = chunks_of(&bytes, 100);
    // The first two chunks in bundle 0xB1, after 7 bytes of another chunk; the third in a bundle the
    // mirror does not have.
    let mut bundle = vec![0xEE; 7];
    for (chunk, frame) in &mut chunks[..2] {
        chunk.place.bundle = 0xB1;
        chunk.place.offset = bundle.len() as u64;
        chunk.place.compressed_size = frame.len() as u32;
        bundle.extend(&*frame);
    }
    chunks[2].0.place.bundle = 0xB2;
    let mirror = BundleMirror::new(&root);
    let path = mirror.bundle_path(0xB1);
    assert_eq!(path, root.join("channels").join("public").join("bundles").join("00000000000000B1.bundle"));
    fs_err::create_dir_all(path.parent().unwrap()).unwrap();
    fs_err::write(&path, &bundle).unwrap();

    let refs: Vec<ChunkRef> = chunks.iter().map(|(c, _)| *c).collect();
    assert!(matches!(mirror.chunks(&refs[2..]), Err(Error::MissingChunk(_))));
    let (memory, _) = file_of(&bytes, 100);
    let layers = Layers::new(vec![&mirror, &memory]);
    let mut file = FileReader::new(&layers, refs);
    assert_eq!(file.read_range(0, 300).unwrap(), bytes);
}
