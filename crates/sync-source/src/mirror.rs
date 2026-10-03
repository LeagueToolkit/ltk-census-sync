//! Riot's bundles kept whole, at the paths the CDN serves them from
//! (`channels/public/bundles/<BUNDLE ID>.bundle` under a root), so the directory can be served as
//! a mirror of the CDN. A chunk is read where the manifest being read places it, as from the CDN.

use camino::{Utf8Path, Utf8PathBuf};

use crate::bundle::read_exact_at;
use crate::chunk::ChunkSource;
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

    /// Where a bundle is kept, the CDN's path for it under the root.
    pub fn bundle_path(&self, bundle: u64) -> Utf8PathBuf {
        self.root.join(format!("channels/public/bundles/{bundle:016X}.bundle"))
    }
}

impl ChunkSource for BundleMirror {
    fn frames(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, Error> {
        wanted
            .iter()
            .map(|chunk| {
                let path = self.bundle_path(chunk.place.bundle);
                if !path.is_file() {
                    return Err(Error::MissingChunk(chunk.id));
                }
                let mut frame = vec![0u8; chunk.place.compressed_size as usize];
                read_exact_at(&fs_err::File::open(&path)?, chunk.place.offset, &mut frame)?;
                Ok(frame)
            })
            .collect()
    }
}

/// Sources in order: each chunk from the first that holds it.
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
    fn frames(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, Error> {
        wanted
            .iter()
            .map(|chunk| {
                for layer in &self.layers {
                    match layer.frames(std::slice::from_ref(chunk)) {
                        Ok(mut frames) => return Ok(frames.remove(0)),
                        Err(Error::MissingChunk(_)) => continue,
                        Err(e) => return Err(e),
                    }
                }
                Err(Error::MissingChunk(chunk.id))
            })
            .collect()
    }
}
