//! One bin object read with ltk_meta and printed as C++ ritobin's text (`docs/FORMAT.md`, "A bin's
//! entries"). Both are writers: the history's `.rito` files are ltk_ritobin's `PrintCanonical` at
//! the pinned commit, of an object read by ltk_meta's `BinObject::from_reader` at the same commit.

use std::io::Cursor;

use ltk_hash::BinHash;
use ltk_meta::{Bin, BinObject};
use ltk_ritobin::PrintCanonical as _;

use crate::Error;

/// The first patch whose bins number the complex property kinds the current way (with
/// `WadChunkLink` and `List2`). Measured by walking the objects' bytes under both numberings: every
/// build to 10.7 fits only the legacy one, every build from 10.8.3167615 only the current one.
const MODERN_BINS_FROM: (u16, u16) = (10, 8);

/// Whether a build of this season and patch uses the legacy property-kind numbering.
pub fn legacy_bins(season: u16, patch: u16) -> bool {
    (season, patch) < MODERN_BINS_FROM
}

/// A bin object from its stored bytes (entry hash to end), as the given class, by the build's
/// numbering. A legacy object can read as a current one without an error (legacy `Container` is
/// current `WadChunkLink`), so the numbering comes from the build, and the other one is tried only
/// when the first fails; its error is not the one reported.
pub fn parse_object(bytes: &[u8], class: u32, legacy: bool) -> Result<BinObject, Error> {
    let mut buf = Vec::with_capacity(bytes.len() + 4);
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
    let mut cursor = Cursor::new(&buf[..]);
    match BinObject::from_reader(&mut cursor, BinHash::from(class), legacy) {
        Ok(object) => Ok(object),
        Err(first) => {
            cursor.set_position(0);
            BinObject::from_reader(&mut cursor, BinHash::from(class), !legacy).map_err(|_| Error::Object(first.to_string()))
        }
    }
}

/// A bin entry's `.rito` text: a bin of that one object, version 3, no links. A function of the
/// object's bytes and class alone, so one object is one blob in every bin and build that holds it.
pub fn render_object(bytes: &[u8], class: u32, legacy: bool) -> Result<String, Error> {
    let object = parse_object(bytes, class, legacy)?;
    let mut bin = Bin::default();
    bin.objects.insert(object.path_hash, object);
    bin.print_canonical().map_err(|e| Error::Object(e.to_string()))
}
