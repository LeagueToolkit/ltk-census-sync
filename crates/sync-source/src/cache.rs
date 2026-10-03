//! The chunk cache (`docs/SOURCES.md`, "Chunk sources"): the frames the CDN gave, in one fjall
//! database. A frame's key is the three things a chunk is: the hash its id is under, its
//! uncompressed size and its id. A frame kept for one chunk is never handed out for a chunk of
//! another scheme or size that shares its id, and every frame read is checked against all three
//! again.

use camino::Utf8Path;
use fjall::{Database, Keyspace, KeyspaceCreateOptions, KvSeparationOptions};

use crate::chunk::{open, ChunkSource};
use crate::rman::ChunkRef;
use crate::Error;

/// Frames fetched from the CDN, on local disk.
#[derive(Clone)]
pub struct ChunkCache {
    // The database's handle runs the flushes and compactions; dropping the last one stops them,
    // whatever keyspace handles are left.
    _db: Database,
    frames: Keyspace,
}

impl ChunkCache {
    /// The cache under `root`, made if it is not there. One process holds it at a time.
    pub fn open(root: &Utf8Path) -> Result<Self, Error> {
        let db = Database::builder(root.as_std_path()).open()?;
        // Frames are zstd already and a few kilobytes to a megabyte each: kept apart from the
        // keys, uncompressed.
        let frames = db.keyspace("frames", || KeyspaceCreateOptions::default().with_kv_separation(Some(KvSeparationOptions::default())))?;
        Ok(Self { _db: db, frames })
    }

    /// Keeps a chunk's frame. The frame has been checked against the chunk.
    pub fn put(&self, chunk: &ChunkRef, frame: &[u8]) -> Result<(), Error> {
        self.frames.insert(key(chunk), frame)?;
        Ok(())
    }

    /// The frame kept for a chunk, unchecked.
    pub fn frame(&self, chunk: &ChunkRef) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.frames.get(key(chunk))?.map(|frame| frame.to_vec()))
    }
}

/// A frame's key: the chunking parameter version of the hash the chunk's id is under, then its
/// uncompressed size and its id, big-endian.
fn key(chunk: &ChunkRef) -> [u8; 13] {
    let mut key = [0u8; 13];
    key[0] = chunk.hash.version();
    key[1..5].copy_from_slice(&chunk.place.uncompressed_size.to_be_bytes());
    key[5..].copy_from_slice(&chunk.id.to_be_bytes());
    key
}

impl ChunkSource for ChunkCache {
    fn chunks(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, Error> {
        wanted
            .iter()
            .map(|chunk| {
                let frame = self.frames.get(key(chunk))?.ok_or(Error::MissingChunk(chunk.id))?;
                open(chunk, &frame)
            })
            .collect()
    }
}
