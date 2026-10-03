//! A WAD's header and table of contents, read from the front of the file, and an entry's bytes
//! from its stored ones.
//!
//! Every WAD of every build from 8.20 is version 3 (3.0 to 9.x, 3.3 by 12.1, 3.4 at 16.19); another
//! major version is refused. The header is `RW`, major, minor, a 256-byte signature, a u64
//! checksum and an i32 entry count (272 bytes), and the table follows in 32-byte entries: a u64
//! path hash, u32 offset, u32 stored size, u32 size, a byte of compression type and frame count,
//! the duplicate flag, the first frame and the u64 checksum, which is the first eight bytes of
//! the SHA-256 of the stored bytes through 3.3 and their XXH3 from 3.4.

use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};

pub use ltk_wad::WadChunkCompression;
use ltk_wad::WadChunk;

use crate::Error;

const HEADER_LEN: u64 = 272;
const ENTRY_LEN: usize = 32;

/// A WAD's version and entry count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WadHeader {
    pub major: u8,
    pub minor: u8,
    pub entry_count: u32,
}

/// One entry of a WAD's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WadEntry {
    /// XXH64 of the entry's lowercased path.
    pub path_hash: u64,
    /// Where its stored bytes are in the WAD.
    pub offset: u64,
    pub stored_size: u64,
    /// Its size decompressed.
    pub size: u64,
    pub compression: WadChunkCompression,
    pub checksum: u64,
}

impl WadEntry {
    /// Whether the table stores the entry as a link to another file (ltk_wad's `Satellite`).
    pub fn is_link(&self) -> bool {
        self.compression == WadChunkCompression::Satellite
    }
}

/// A WAD's header and table, read from the front of the file.
pub fn read_wad_table(reader: &mut (impl Read + Seek)) -> Result<(WadHeader, Vec<WadEntry>), Error> {
    let fail = |message: String| Error::Wad(message);
    let size = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut head = [0u8; 4];
    reader.read_exact(&mut head)?;
    if &head[..2] != b"RW" {
        return Err(fail(format!("magic {:02x?}", &head[..2])));
    }
    let (major, minor) = (head[2], head[3]);
    if major != 3 {
        return Err(fail(format!("version {major}.{minor}; every WAD from 8.20 is 3.x")));
    }
    reader.seek(SeekFrom::Start(HEADER_LEN - 4))?;
    let mut count = [0u8; 4];
    reader.read_exact(&mut count)?;
    let count = i32::from_le_bytes(count);
    let entry_count = u32::try_from(count).map_err(|_| fail(format!("entry count {count}")))?;
    let table_len = ENTRY_LEN as u64 * u64::from(entry_count);
    if HEADER_LEN + table_len > size {
        return Err(fail(format!("a table of {entry_count} entries runs past the end of {size} bytes")));
    }
    let mut table = vec![0u8; table_len as usize];
    reader.read_exact(&mut table)?;
    let entries = table
        .as_chunks::<ENTRY_LEN>()
        .0
        .iter()
        .map(|row| {
            let mut row = BufReader::new(Cursor::new(row));
            let chunk = if minor >= 4 { WadChunk::read_v3_4(&mut row) } else { WadChunk::read_v3_1(&mut row) };
            let chunk = chunk.map_err(|e| fail(format!("the table: {e}")))?;
            Ok(WadEntry {
                path_hash: chunk.path_hash.0,
                offset: chunk.data_offset as u64,
                stored_size: chunk.compressed_size as u64,
                size: chunk.uncompressed_size as u64,
                compression: chunk.compression_type,
                checksum: chunk.checksum,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok((WadHeader { major, minor, entry_count }, entries))
}

/// An entry's bytes from its stored ones: decompressed, or for a link the stored bytes as they
/// are, which hold the path of the file it stands for.
pub fn entry_bytes(stored: &[u8], entry: &WadEntry) -> Result<Vec<u8>, Error> {
    if entry.is_link() {
        return Ok(stored.to_vec());
    }
    ltk_wad::decompress_raw(stored, entry.compression, entry.size as usize)
        .map(Vec::from)
        .map_err(|e| Error::Wad(format!("entry {:016x}: {e}", entry.path_hash)))
}
