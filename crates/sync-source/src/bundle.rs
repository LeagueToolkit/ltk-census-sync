//! A local merged bundle (`docs/SOURCES.md`, "Chunk sources"): the form an archive of past builds
//! keeps, one bundle split in parts, read through its own tables and never by a manifest's offsets,
//! which describe another encoding. A chunk is looked up by its id and uncompressed size, and each
//! copy the tables list of that id and size is tried until one checks under the chunk's hash: an
//! archive can hold a frame under the wrong id (`docs/RUNS.md`).
//!
//! A part is a flat run of zstd frames, one per chunk, then a table of contents (16 bytes a chunk:
//! u64 id, u32 uncompressed size, u32 compressed size) and a 20-byte footer (u64 checksum, u32
//! count, u32 version, `RBUN`). The merged form's version is `0xFFFFFFFF`, and its checksum is
//! the XXH64 of the table, where Riot's bundles hold the bundle id. Parts are `<name>.bundle`,
//! `<name>.00001.bundle`, ..., and the next part is looked for while one exists.

use camino::{Utf8Path, Utf8PathBuf};
use xxhash_rust::xxh64::xxh64;

use crate::chunk::{open, ChunkSource};
use crate::rman::ChunkRef;
use crate::Error;

const FOOTER_LEN: u64 = 20;
const TOC_ENTRY_LEN: u64 = 16;
const MERGED_VERSION: u32 = 0xFFFF_FFFF;
const RIOT_VERSION: u32 = 1;

/// Where a copy of a chunk's frame is, and the size its table gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Place {
    part: usize,
    offset: u64,
    compressed_size: u32,
    uncompressed_size: u32,
}

struct Part {
    path: Utf8PathBuf,
    file: fs_err::File,
    size: u64,
}

/// A merged bundle, open for positional reads from any thread.
pub struct MergedBundle {
    parts: Vec<Part>,
    /// Every copy its tables list, by chunk id, sorted; the copies of one id in part order.
    ids: Vec<u64>,
    places: Vec<Place>,
}

impl MergedBundle {
    /// Opens every part from the first, and reads their tables.
    pub fn open(first: &Utf8Path) -> Result<Self, Error> {
        let mut parts = Vec::new();
        let mut path = first.to_path_buf();
        while path.is_file() || parts.is_empty() {
            let file = fs_err::File::open(&path)?;
            let size = file.metadata()?.len();
            parts.push(Part { path, file, size });
            path = first.with_extension(format!("{:05}.bundle", parts.len()));
        }
        let mut pairs: Vec<(u64, Place)> = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            let mut offset = 0;
            for (id, uncompressed_size, compressed_size) in read_toc(part)? {
                pairs.push((id, Place { part: index, offset, compressed_size, uncompressed_size }));
                offset += u64::from(compressed_size);
            }
        }
        // Stable, so the copies of one id stay in part order.
        pairs.sort_by_key(|(id, _)| *id);
        let (ids, places) = pairs.into_iter().unzip();
        Ok(Self { parts, ids, places })
    }

    /// The number of chunk copies its tables list.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether the bundle holds no chunk.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// A chunk from the first copy of its id, of its uncompressed size, whose bytes check under its
    /// hash.
    fn chunk(&self, chunk: &ChunkRef) -> Result<Vec<u8>, Error> {
        let from = self.ids.partition_point(|&id| id < chunk.id);
        let to = self.ids.partition_point(|&id| id <= chunk.id);
        let mut first = None;
        for place in self.places[from..to].iter().filter(|p| p.uncompressed_size == chunk.place.uncompressed_size) {
            let mut frame = vec![0u8; place.compressed_size as usize];
            read_exact_at(&self.parts[place.part].file, place.offset, &mut frame)?;
            match open(chunk, &frame) {
                Ok(data) => return Ok(data),
                Err(e) => {
                    first.get_or_insert(e);
                }
            }
        }
        Err(first.unwrap_or(Error::MissingChunk(chunk.id)))
    }
}

impl ChunkSource for MergedBundle {
    fn chunks(&self, wanted: &[ChunkRef]) -> Result<Vec<Vec<u8>>, Error> {
        wanted.iter().map(|chunk| self.chunk(chunk)).collect()
    }
}

/// A part's table: each chunk's id, uncompressed size and compressed size, in stored order. Checks the merged form's
/// checksum, and that the frames end where the table starts.
fn read_toc(part: &Part) -> Result<Vec<(u64, u32, u32)>, Error> {
    let fail = |message: String| Error::Bundle { path: part.path.to_string(), message };
    if part.size < FOOTER_LEN {
        return Err(fail("shorter than a footer".into()));
    }
    let mut footer = [0u8; FOOTER_LEN as usize];
    read_exact_at(&part.file, part.size - FOOTER_LEN, &mut footer)?;
    if &footer[16..] != b"RBUN" {
        return Err(fail(format!("the footer ends {:02x?}, not RBUN", &footer[16..])));
    }
    let checksum = u64::from_le_bytes(footer[..8].try_into().expect("eight bytes"));
    let count = u32::from_le_bytes(footer[8..12].try_into().expect("four bytes"));
    let version = u32::from_le_bytes(footer[12..16].try_into().expect("four bytes"));
    if version != MERGED_VERSION && version != RIOT_VERSION {
        return Err(fail(format!("version {version:#x}")));
    }
    let toc_len = u64::from(count) * TOC_ENTRY_LEN;
    let toc_offset = part
        .size
        .checked_sub(FOOTER_LEN + toc_len)
        .ok_or_else(|| fail(format!("a table of {count} chunks runs past the start")))?;
    let mut toc = vec![0u8; toc_len as usize];
    read_exact_at(&part.file, toc_offset, &mut toc)?;
    if version == MERGED_VERSION && xxh64(&toc, 0) != checksum {
        return Err(fail(format!("the table hashes to {:016x}, the footer says {checksum:016x}", xxh64(&toc, 0))));
    }
    let rows: Vec<(u64, u32, u32)> = toc
        .as_chunks::<16>()
        .0
        .iter()
        .map(|row| {
            let field = |at: usize| u32::from_le_bytes(row[at..at + 4].try_into().expect("four bytes"));
            (u64::from_le_bytes(row[..8].try_into().expect("eight bytes")), field(8), field(12))
        })
        .collect();
    let frames_end: u64 = rows.iter().map(|&(_, _, size)| u64::from(size)).sum();
    if frames_end != toc_offset {
        return Err(fail(format!("the frames end at {frames_end}, the table starts at {toc_offset}")));
    }
    Ok(rows)
}

/// Fills `buf` from `offset` without moving a cursor another thread shares.
pub(crate) fn read_exact_at(file: &fs_err::File, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use fs_err::os::windows::fs::FileExt;
        let mut done = 0;
        while done < buf.len() {
            let n = file.seek_read(&mut buf[done..], offset + done as u64)?;
            if n == 0 {
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, format!("{}: short read at {offset}", file.path().display())));
            }
            done += n;
        }
        Ok(())
    }
    #[cfg(unix)]
    {
        use fs_err::os::unix::fs::FileExt;
        file.read_exact_at(buf, offset)
    }
}
