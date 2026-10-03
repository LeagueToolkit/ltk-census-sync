//! What the tests build: WADs, manifests of them, and a scratch history.

use std::collections::HashMap;
use std::io::Write;
use std::process::Command;

use camino::Utf8PathBuf;
use ltk_wad::{WadChunk, WadChunkCompression, WadHash};
use sync_format::BuildFacts;
use sync_history::Git;
use sync_source::{open_frame, BundleChunk, ChunkHash, ChunkRef, ChunkSource, ChunkingParams, Manifest, ManifestFile};

/// Frames by chunk id.
#[derive(Default)]
pub struct Frames(pub HashMap<u64, Vec<u8>>);

impl ChunkSource for Frames {
    fn chunks(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, sync_source::Error> {
        let mut dec = zstd::bulk::Decompressor::new()?;
        wanted.iter().map(|c| open_frame(c, self.0.get(&c.id).ok_or(sync_source::Error::MissingChunk(c.id))?, &mut dec)).collect()
    }
}

/// A v3.4 WAD of uncompressed entries: (path hash, checksum, bytes).
pub fn wad(entries: &[(u64, u64, &[u8])]) -> Vec<u8> {
    let mut bytes = b"RW\x03\x04".to_vec();
    bytes.extend([0u8; 264]);
    bytes.extend((entries.len() as i32).to_le_bytes());
    let mut offset = bytes.len() + 32 * entries.len();
    for (hash, checksum, data) in entries {
        let chunk = WadChunk {
            path_hash: WadHash(*hash),
            data_offset: offset,
            compressed_size: data.len(),
            uncompressed_size: data.len(),
            compression_type: WadChunkCompression::None,
            is_duplicated: false,
            frame_count: 0,
            start_frame: 0,
            checksum: *checksum,
        };
        chunk.write_v3_4(&mut bytes).unwrap();
        offset += data.len();
    }
    for (_, _, data) in entries {
        bytes.extend(*data);
    }
    bytes
}

/// A manifest of WADs, each one chunk, and the frames of those whose bytes are given.
pub fn manifest(wads: &[(&str, u64, Vec<u8>, bool)]) -> (Manifest, Frames) {
    let (mut files, mut places, mut frames) = (Vec::new(), HashMap::new(), Frames::default());
    for (path, id, bytes, available) in wads {
        let chunk = ChunkHash::Blake3.id_of(bytes);
        places.insert(chunk, BundleChunk { bundle: 1, offset: 0, compressed_size: 0, uncompressed_size: bytes.len() as u32 });
        if *available {
            frames.0.insert(chunk, zstd::bulk::compress(bytes, 1).unwrap());
        }
        files.push(ManifestFile { id: *id, path: path.to_string(), size: bytes.len() as u64, tags: Vec::new(), params_id: 1, chunk_ids: vec![chunk] });
    }
    (Manifest::new(0xabc, vec![ChunkingParams { id: 1, version: 4 }], files, places), frames)
}

pub fn own(dir: &str, hash: u64) -> String {
    format!("{dir}/{:02x}/{hash:016x}.yaml", hash >> 56)
}

pub struct Scratch {
    _dir: tempfile::TempDir,
    pub git: Git,
}

/// A bare repository whose `history` is one commit of these files.
pub fn history(files: &[(String, String)]) -> Scratch {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("history.git")).unwrap();
    assert!(Command::new("git").args(["init", "--bare", "-q", path.as_str()]).status().unwrap().success());
    let git = Git::open(&path).unwrap();
    let mut import = git.fast_import().unwrap();
    let out = import.stream();
    write!(out, "commit refs/heads/history\ncommitter census <census@localhost> 0 +0000\ndata 5\nbase\n").unwrap();
    for (path, text) in files {
        write!(out, "M 100644 inline {path}\ndata {}\n{text}\n", text.len()).unwrap();
    }
    import.finish().unwrap();
    Scratch { _dir: dir, git }
}


pub fn facts() -> BuildFacts {
    BuildFacts {
        version: "16.20.1".into(),
        patch: "16.20".into(),
        manifest: 0xabc,
        rads: None,
        date: "2026-10-01".into(),
        legacy_bins: false,
        realms: vec!["NA1".into(), "EUW1".into()],
    }
}
