//! Riot's CDN as a chunk source of last resort: a chunk whose bundle the mirror lacks has its whole
//! bundle downloaded into the mirror, once, and is read from there. Whole bundles keep the mirror
//! servable as a CDN; ranges of them are for when a run has no mirror (`docs/ROADMAP.md`).

use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

use camino::Utf8Path;

use crate::chunk::ChunkSource;
use crate::mirror::{bundle_url_path, BundleMirror};
use crate::rman::ChunkRef;
use crate::Error;

/// Riot's bundle host, over https.
pub const BUNDLE_HOST: &str = "https://lol.dyn.riotcdn.net";

/// A CDN that fills a mirror.
pub struct Cdn {
    host: String,
    mirror: BundleMirror,
    agent: ureq::Agent,
    /// Held while a bundle downloads, so two threads wanting it fetch it once.
    downloading: Mutex<()>,
}

impl Cdn {
    /// A CDN at `host` (`BUNDLE_HOST`, or a mirror served over HTTP) that fills the mirror under
    /// `root`.
    pub fn new(host: &str, root: &Utf8Path) -> Self {
        let agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(600))).build().into();
        Self { host: host.trim_end_matches('/').to_string(), mirror: BundleMirror::new(root), agent, downloading: Mutex::new(()) }
    }

    /// Downloads a bundle into the mirror unless it is there. The bundle is written to a `.part`
    /// file and moved into place once its footer names it.
    pub fn fetch(&self, bundle: u64) -> Result<(), Error> {
        let path = self.mirror.bundle_path(bundle);
        let _held = self.downloading.lock().expect("a download thread does not panic");
        if path.is_file() {
            return Ok(());
        }
        let url = format!("{}/{}", self.host, bundle_url_path(bundle));
        let fail = |message: String| Error::Download { url: url.clone(), message };
        let mut response = self.agent.get(&url).call().map_err(|e| fail(e.to_string()))?;
        if let Some(dir) = path.parent() {
            fs_err::create_dir_all(dir)?;
        }
        let part = path.with_extension("bundle.part");
        let mut file = fs_err::File::create(&part)?;
        let mut body = response.body_mut().with_config().limit(u64::MAX).reader();
        std::io::copy(&mut body, &mut file).map_err(|e| fail(e.to_string()))?;
        file.flush()?;
        drop(file);
        let bytes = fs_err::read(&part)?;
        let footer = bytes.len().checked_sub(20).map(|at| &bytes[at..]).ok_or_else(|| fail("shorter than a footer".into()))?;
        let named = u64::from_le_bytes(footer[..8].try_into().expect("eight bytes"));
        if &footer[16..] != b"RBUN" || named != bundle {
            fs_err::remove_file(&part)?;
            return Err(fail(format!("the footer names bundle {named:016X}, not this one")));
        }
        fs_err::rename(&part, &path)?;
        Ok(())
    }
}

impl ChunkSource for Cdn {
    fn chunks(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, Error> {
        wanted
            .iter()
            .map(|chunk| {
                self.fetch(chunk.place.bundle)?;
                Ok(self.mirror.chunks(std::slice::from_ref(chunk))?.remove(0))
            })
            .collect()
    }
}
