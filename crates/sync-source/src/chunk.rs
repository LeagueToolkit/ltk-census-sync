//! Chunks by id (`docs/SOURCES.md`, "Chunk sources"). A source hands back each chunk's stored
//! frame, found its own way; every frame is checked here, after decompression, against the size
//! and the id the manifest gives. Identity is the id alone: the same chunk can be compressed
//! differently in different bundles, so a frame's compressed size is never compared.

use crate::rman::ChunkRef;
use crate::Error;

/// Where chunks come from.
pub trait ChunkSource: Sync {
    /// Each wanted chunk's stored frame, a zstd frame, in the order asked. Asking for several at
    /// once lets a source batch its reads.
    fn frames(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, Error>;
}

/// A chunk's bytes from its frame: decompressed, and checked against its size and its id.
pub fn open_frame(chunk: &ChunkRef, frame: &[u8], dec: &mut zstd::bulk::Decompressor<'_>) -> Result<Vec<u8>, Error> {
    let bad = |message: String| Error::BadChunk { id: chunk.id, message };
    let size = chunk.place.uncompressed_size as usize;
    let data = dec.decompress(frame, size).map_err(|e| bad(format!("does not decompress: {e}")))?;
    if data.len() != size {
        return Err(bad(format!("decompresses to {} bytes, the manifest says {size}", data.len())));
    }
    let id = chunk.hash.id_of(&data);
    if id != chunk.id {
        return Err(bad(format!("hashes to {id:016x} under {:?}", chunk.hash)));
    }
    Ok(data)
}
