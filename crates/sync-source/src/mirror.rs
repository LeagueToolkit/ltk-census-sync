//! Riot's bundles kept whole, at the paths the CDN serves them from
//! (`channels/public/bundles/<BUNDLE ID>.bundle` under a root), so the directory can be served as
//! a mirror of the CDN. A chunk is read where the manifest being read places it, as from the CDN.

use std::io::{Read, Seek, SeekFrom};

use camino::{Utf8Path, Utf8PathBuf};

use crate::chunk::{open, ChunkSource};
use crate::rman::ChunkRef;
use crate::Error;

/// A directory of whole bundles, laid out as the CDN.
pub struct BundleMirror {
    root: Utf8PathBuf,
}

impl BundleMirror {
    /// The mirror under `root`.
    pub fn new(root: &Utf8Path) -> Self {
        Self { root: root.to_path_buf() }
    }

    /// Where a bundle is kept: the CDN's path for it, under the root.
    pub fn bundle_path(&self, bundle: u64) -> Utf8PathBuf {
        self.root.join(bundle_url_path(bundle))
    }
}

/// A bundle's path on the CDN, under its host.
pub(crate) fn bundle_url_path(bundle: u64) -> String {
    format!("channels/public/bundles/{bundle:016X}.bundle")
}

impl ChunkSource for BundleMirror {
    fn chunks(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, Error> {
        wanted
            .iter()
            .map(|chunk| {
                let path = self.bundle_path(chunk.place.bundle);
                if !path.is_file() {
                    return Err(Error::MissingChunk(chunk.id));
                }
                let mut frame = vec![0u8; chunk.place.compressed_size as usize];
                let mut file = fs_err::File::open(&path)?;
                file.seek(SeekFrom::Start(chunk.place.offset))?;
                file.read_exact(&mut frame)?;
                open(chunk, &frame)
            })
            .collect()
    }
}

/// Sources in order: each chunk from the first that holds it with bytes that check. A source that
/// lacks a chunk, or holds bytes for it that do not check, passes it to the next; when none has it,
/// the first source's error is the one returned.
pub struct Layers<'a> {
    layers: Vec<&'a dyn ChunkSource>,
}

impl<'a> Layers<'a> {
    /// These sources, the first asked first.
    pub fn new(layers: Vec<&'a dyn ChunkSource>) -> Self {
        Self { layers }
    }
}

impl ChunkSource for Layers<'_> {
    fn chunks(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, Error> {
        wanted
            .iter()
            .map(|chunk| {
                let mut first = None;
                for layer in &self.layers {
                    match layer.chunks(std::slice::from_ref(chunk)) {
                        Ok(mut data) => return Ok(data.remove(0)),
                        Err(e @ (Error::MissingChunk(_) | Error::BadChunk { .. })) => {
                            first.get_or_insert(e);
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(first.unwrap_or(Error::MissingChunk(chunk.id)))
            })
            .collect()
    }
}
