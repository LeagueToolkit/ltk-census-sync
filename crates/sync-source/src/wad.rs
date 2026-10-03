//! A WAD's header and table of contents, read from the front of the file, and an entry's bytes
//! from its stored ones.
//!
//! Three header layouts:
//!
//! - v1: `RW`, major, minor, u16 table offset, u16 entry size, u32 entry count (12 bytes);
//! - v2: `RW`, major, minor, an 84-byte signature, a u64 checksum, then v1's three fields (104);
//! - v3: `RW`, major, minor, a 256-byte signature, a u64 checksum, an i32 entry count (272), the
//!   table right after it in 32-byte entries.
//!
//! An entry is a u64 path hash, u32 offset, u32 stored size, u32 size, a byte of compression type
//! and frame count, then (v3, and v2's 32-byte entries) the duplicate flag, the first frame and
//! the u64 checksum: the first eight bytes of the SHA-256 of the stored bytes through v3.3, their
//! XXH3 from v3.4. v1 entries are 24 bytes and have no checksum.

use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};

pub use ltk_wad::WadChunkCompression;
use ltk_wad::WadChunk;

use crate::Error;

/// A WAD's header, without its signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WadHeader {
    pub major: u8,
    pub minor: u8,
    pub entry_count: u32,
    /// Where the table starts.
    pub table_offset: u32,
    /// The size of one entry of the table.
    pub entry_size: u32,
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
    /// None in a WAD whose table has no checksums.
    pub checksum: Option<u64>,
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
    reader.seek(SeekFrom::Start(0))?;
    let mut head = vec![0u8; 4];
    reader.read_exact(&mut head)?;
    if &head[..2] != b"RW" {
        return Err(fail(format!("magic {:02x?}", &head[..2])));
    }
    let (major, minor) = (head[2], head[3]);
    let header_len = match major {
        1 => 12,
        2 => 104,
        3 => 272,
        _ => return Err(fail(format!("version {major}.{minor} is not 1.x, 2.x or 3.x"))),
    };
    head.resize(header_len, 0);
    reader.read_exact(&mut head[4..])?;
    let u16_at = |at: usize| u32::from(u16::from_le_bytes(head[at..at + 2].try_into().expect("two bytes")));
    let i32_at = |at: usize| i32::from_le_bytes(head[at..at + 4].try_into().expect("four bytes"));
    let (table_offset, entry_size, count) = match major {
        1 => (u16_at(4), u16_at(6), i32_at(8)),
        2 => (u16_at(96), u16_at(98), i32_at(100)),
        _ => (272, 32, i32_at(268)),
    };
    let entry_count = u32::try_from(count).map_err(|_| fail(format!("entry count {count}")))?;
    if entry_size < 24 || (table_offset as usize) < header_len {
        return Err(fail(format!("{major}.{minor}: a table at {table_offset} in {entry_size}-byte entries")));
    }
    let header = WadHeader { major, minor, entry_count, table_offset, entry_size };

    let table_len = u64::from(entry_size) * u64::from(entry_count);
    let size = reader.seek(SeekFrom::End(0))?;
    if u64::from(table_offset) + table_len > size {
        return Err(fail(format!("a table of {entry_count} entries runs past the end of {size} bytes")));
    }
    let mut table = vec![0u8; table_len as usize];
    reader.seek(SeekFrom::Start(u64::from(table_offset)))?;
    reader.read_exact(&mut table)?;
    let mut entries = Vec::with_capacity(entry_count as usize);
    for row in table.chunks_exact(entry_size as usize) {
        let entry = if row.len() >= 32 {
            let mut row = BufReader::new(Cursor::new(&row[..32]));
            let chunk = match (major, minor) {
                (3, 4..) => WadChunk::read_v3_4(&mut row),
                _ => WadChunk::read_v3_1(&mut row),
            }
            .map_err(|e| fail(format!("the table: {e}")))?;
            WadEntry {
                path_hash: chunk.path_hash.0,
                offset: chunk.data_offset as u64,
                stored_size: chunk.compressed_size as u64,
                size: chunk.uncompressed_size as u64,
                compression: chunk.compression_type,
                checksum: Some(chunk.checksum),
            }
        } else {
            let u32_at = |at: usize| u64::from(u32::from_le_bytes(row[at..at + 4].try_into().expect("four bytes")));
            WadEntry {
                path_hash: u64::from_le_bytes(row[..8].try_into().expect("eight bytes")),
                offset: u32_at(8),
                stored_size: u32_at(12),
                size: u32_at(16),
                compression: WadChunkCompression::try_from(row[20] & 0xF)
                    .map_err(|_| fail(format!("the table: compression {}", row[20] & 0xF)))?,
                checksum: None,
            }
        };
        entries.push(entry);
    }
    Ok((header, entries))
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
