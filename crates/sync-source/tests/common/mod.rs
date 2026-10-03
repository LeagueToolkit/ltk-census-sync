//! Chunks the tests make: bytes cut into chunks, with their frames.

use std::collections::HashMap;

use sync_source::{BundleChunk, ChunkHash, ChunkRef, ChunkSource, Error};

/// Frames by chunk id, in memory.
#[derive(Default)]
pub struct MemorySource {
    pub frames: HashMap<u64, Vec<u8>>,
}

impl ChunkSource for MemorySource {
    fn frames(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, Error> {
        wanted.iter().map(|c| self.frames.get(&c.id).cloned().ok_or(Error::MissingChunk(c.id))).collect()
    }
}

/// `bytes` cut into chunks of `size`: each chunk's reference and its frame. The places are made
/// up; only the uncompressed sizes are read.
pub fn chunks_of(bytes: &[u8], size: usize) -> Vec<(ChunkRef, Vec<u8>)> {
    bytes
        .chunks(size)
        .map(|data| {
            let place = BundleChunk { bundle: 1, offset: 0, compressed_size: 0, uncompressed_size: data.len() as u32 };
            let chunk = ChunkRef { id: ChunkHash::Blake3.id_of(data), hash: ChunkHash::Blake3, place };
            (chunk, zstd::bulk::compress(data, 1).unwrap())
        })
        .collect()
}

/// A source holding `bytes` in chunks of `size`, and the file's chunk references.
pub fn file_of(bytes: &[u8], size: usize) -> (MemorySource, Vec<ChunkRef>) {
    let chunks = chunks_of(bytes, size);
    let refs = chunks.iter().map(|(c, _)| *c).collect();
    (MemorySource { frames: chunks.into_iter().map(|(c, f)| (c.id, f)).collect() }, refs)
}
