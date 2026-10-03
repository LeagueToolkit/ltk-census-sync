//! The chunk cache: a frame is kept under its chunk's hash scheme, uncompressed size and id.

use camino::Utf8PathBuf;
use sync_source::{BundleChunk, ChunkCache, ChunkHash, ChunkRef, ChunkSource, Error};

/// `bytes` cut into BLAKE3 chunks of `size`, with their frames.
fn chunks_of(bytes: &[u8], size: usize) -> Vec<(ChunkRef, Vec<u8>)> {
    bytes
        .chunks(size)
        .map(|data| {
            let place = BundleChunk { bundle: 1, offset: 0, compressed_size: 0, uncompressed_size: data.len() as u32 };
            (ChunkRef { id: ChunkHash::Blake3.id_of(data), hash: ChunkHash::Blake3, place }, zstd::bulk::compress(data, 1).unwrap())
        })
        .collect()
}

#[test]
fn a_frame_is_found_under_its_hash_scheme_size_and_id_only_and_kept_across_opens() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().join("chunks")).unwrap();
    let (chunk, frame) = chunks_of(b"census", 6).remove(0);
    {
        let cache = ChunkCache::open(&root).unwrap();
        cache.put(&chunk, &frame).unwrap();
        assert_eq!(cache.chunks(&[chunk]).unwrap(), [b"census".to_vec()]);
        let mut other_scheme = chunk;
        other_scheme.hash = ChunkHash::RitoHkdf;
        let mut other_size = chunk;
        other_size.place.uncompressed_size = 7;
        for other in [other_scheme, other_size] {
            assert_eq!(cache.frame(&other).unwrap(), None);
            assert!(matches!(cache.chunks(&[other]), Err(Error::MissingChunk(id)) if id == chunk.id));
        }
    }
    let cache = ChunkCache::open(&root).unwrap();
    assert_eq!(cache.frame(&chunk).unwrap(), Some(frame));
}

#[test]
fn a_frame_kept_for_another_chunk_does_not_check() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ChunkCache::open(&Utf8PathBuf::from_path_buf(dir.path().join("chunks")).unwrap()).unwrap();
    let chunks = chunks_of(b"censussync__", 6);
    cache.put(&chunks[1].0, &chunks[0].1).unwrap();
    assert!(matches!(cache.chunks(&[chunks[1].0]), Err(Error::BadChunk { id, .. }) if id == chunks[1].0.id));
}
