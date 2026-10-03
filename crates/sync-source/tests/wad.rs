mod common;

use std::io::Cursor;

use common::file_of;
use ltk_wad::{WadChunk, WadHash};
use sync_source::{entry_bytes, read_wad_table, Error, FileReader, WadChunkCompression, WadEntry};

/// A v3.4 WAD of these entries: (path hash, compression, bytes as stored, size).
fn wad_v3_4(entries: &[(u64, WadChunkCompression, Vec<u8>, usize)]) -> Vec<u8> {
    let mut bytes = b"RW\x03\x04".to_vec();
    bytes.extend([0u8; 256]);
    bytes.extend(0x5555u64.to_le_bytes());
    bytes.extend((entries.len() as i32).to_le_bytes());
    let mut offset = bytes.len() + 32 * entries.len();
    for (i, (hash, compression, stored, size)) in entries.iter().enumerate() {
        let chunk = WadChunk {
            path_hash: WadHash(*hash),
            data_offset: offset,
            compressed_size: stored.len(),
            uncompressed_size: *size,
            compression_type: *compression,
            is_duplicated: false,
            frame_count: 0,
            start_frame: 0,
            checksum: 0xC0 + i as u64,
        };
        chunk.write_v3_4(&mut bytes).unwrap();
        offset += stored.len();
    }
    for (_, _, stored, _) in entries {
        bytes.extend(stored);
    }
    bytes
}

#[test]
fn a_wad_is_read_from_its_table_and_each_entry_from_its_stored_bytes() {
    let bin = b"PROP\x03\0\0\0\0\0\0\0\0\0\0\0".repeat(20);
    let link = b"\x0e\0\0\0data/maps/x.bin".to_vec();
    let entries = [
        (0x10, WadChunkCompression::Zstd, zstd::bulk::compress(&bin, 3).unwrap(), bin.len()),
        (0x20, WadChunkCompression::None, b"plain".to_vec(), 5),
        (0x30, WadChunkCompression::Satellite, link.clone(), 0),
    ];
    let wad = wad_v3_4(&entries);
    let (source, chunks) = file_of(&wad, 50);
    let mut file = FileReader::new(&source, chunks);
    let (header, table) = read_wad_table(&mut file).unwrap();
    assert_eq!((header.major, header.minor, header.entry_count), (3, 4, 3));
    assert_eq!(
        table[1],
        WadEntry { path_hash: 0x20, offset: 368 + entries[0].2.len() as u64, stored_size: 5, size: 5, compression: WadChunkCompression::None, checksum: 0xC1 }
    );
    assert!(table[2].is_link() && !table[0].is_link());
    let read = |file: &mut FileReader<_>, e: &WadEntry| entry_bytes(&file.read_range(e.offset, e.stored_size).unwrap(), e).unwrap();
    assert_eq!(read(&mut file, &table[0]), bin);
    assert_eq!(read(&mut file, &table[1]), b"plain");
    assert_eq!(read(&mut file, &table[2]), link);
}

#[test]
fn a_table_past_the_end_or_another_version_is_refused() {
    let mut bytes = b"RW\x03\x04".to_vec();
    bytes.extend([0u8; 264]);
    bytes.extend(1_000_000i32.to_le_bytes());
    assert!(read_wad_table(&mut Cursor::new(bytes)).is_err());
    // Version 2, which no build from 8.20 ships.
    let mut v2 = b"RW\x02\x00".to_vec();
    v2.extend([0u8; 100]);
    assert!(matches!(read_wad_table(&mut Cursor::new(v2)), Err(Error::Wad(m)) if m.starts_with("version 2.0")));
}
