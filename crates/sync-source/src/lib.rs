//! Where a build's bytes come from (`docs/SOURCES.md`): the live builds to append, their RMAN
//! manifests, and chunks by id from a local cache or Riot's CDN, rebuilt into a file or a range of
//! one through `Read + Seek`.
//!
//! Shaped as a general manifest crate would be, so it can move into the LeagueToolkit family
//! whole: nothing here knows about the history.

mod bundle;
mod chunk;
mod chunk_hash;
mod error;
mod file;
mod rman;
mod wad;

pub use bundle::MergedBundle;
pub use chunk::{open_frame, ChunkSource};
pub use chunk_hash::ChunkHash;
pub use error::Error;
pub use file::FileReader;
pub use rman::{BundleChunk, ChunkRef, ChunkingParams, Manifest, ManifestFile};
pub use wad::{entry_bytes, read_wad_table, WadChunkCompression, WadEntry, WadHeader};
