//! What each file is named (`docs/FORMAT.md`, "Layout"). A name is a stable identity, never
//! content: git sorts objects by a hash of their path before its delta search, so every version of
//! one file lands together.

/// A WAD's directory: `files/` and its manifest path, lowercased.
pub fn wad_dir(manifest_path: &str) -> String {
    format!("files/{}", manifest_path.to_lowercase())
}

/// An entry's name in its WAD's directory, without extension: its path hash in the directory of the
/// hash's first byte. Its own file is `<name>.yaml`, a bin's entries are under `<name>.bin/`.
pub fn entry_name(path_hash: u64) -> String {
    format!("{:02x}/{path_hash:016x}", path_hash >> 56)
}

/// A bin entry's file in its bin's directory: its entry hash in the directory of the hash's first
/// byte. The full hash is the name, so that it is the entry's key in the bin's `objects`.
pub fn object_name(entry_hash: u32) -> String {
    format!("{:02x}/{entry_hash:08x}.rito", entry_hash >> 24)
}
