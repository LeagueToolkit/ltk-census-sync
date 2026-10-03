//! Chunks (`docs/SOURCES.md`, "Chunk sources"). A chunk is its id, the hash its id is under (its
//! file's chunking version) and its uncompressed size, all three from the manifest. A source finds
//! it its own way and hands back its bytes checked against all three after decompression. The same
//! chunk can be compressed differently in different bundles, so a frame's compressed size is never
//! compared.

use std::cell::RefCell;

use crate::rman::ChunkRef;
use crate::Error;

/// Where chunks come from.
pub trait ChunkSource: Sync {
    /// Each wanted chunk's bytes, decompressed and checked, in the order asked. Asking for several
    /// at once lets a source batch its reads. A chunk the source does not hold is
    /// `Error::MissingChunk`; one whose bytes do not check is `Error::BadChunk`.
    fn chunks(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, Error>;
}

/// A chunk's bytes from its stored frame: decompressed, and checked against its size and its id.
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

thread_local! {
    static DECOMPRESSOR: RefCell<Option<zstd::bulk::Decompressor<'static>>> = const { RefCell::new(None) };
}

/// `open_frame` with a decompressor kept per thread, for a store that reads frames.
pub(crate) fn open(chunk: &ChunkRef, frame: &[u8]) -> Result<Vec<u8>, Error> {
    DECOMPRESSOR.with_borrow_mut(|dec| {
        if dec.is_none() {
            *dec = Some(zstd::bulk::Decompressor::new()?);
        }
        open_frame(chunk, frame, dec.as_mut().expect("set above"))
    })
}
