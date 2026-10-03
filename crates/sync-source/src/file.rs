//! A manifest file read as `Read + Seek` over its chunks (`docs/SOURCES.md`, "Files and ranges"):
//! a seek costs nothing, and a read fetches the chunks it overlaps that are not held, in one
//! request to the source. A caller that knows the ranges it will read preloads them, so their
//! chunks come in one request rather than one a read.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Seek, SeekFrom};
use std::ops::Range;

use crate::chunk::ChunkSource;
use crate::rman::ChunkRef;
use crate::Error;

/// One file's bytes, from its chunks.
pub struct FileReader<'a, S: ChunkSource + ?Sized> {
    source: &'a S,
    chunks: Vec<ChunkRef>,
    /// Where each chunk starts; the last value is the file's size.
    starts: Vec<u64>,
    pos: u64,
    /// The chunks of the last preload or of the last read that needed one not held, by index: a
    /// WAD's entries lie in table order, and one chunk often ends one entry and starts the next.
    held: BTreeMap<usize, Vec<u8>>,
}

impl<'a, S: ChunkSource + ?Sized> FileReader<'a, S> {
    /// A file made of these chunks, in file order.
    pub fn new(source: &'a S, chunks: Vec<ChunkRef>) -> Self {
        let mut starts = Vec::with_capacity(chunks.len() + 1);
        starts.push(0);
        for chunk in &chunks {
            starts.push(starts[starts.len() - 1] + u64::from(chunk.place.uncompressed_size));
        }
        Self { source, chunks, starts, pos: 0, held: BTreeMap::new() }
    }

    /// The file's size.
    pub fn size(&self) -> u64 {
        self.starts[self.starts.len() - 1]
    }

    /// The bytes at `[offset, offset + len)`.
    pub fn read_range(&mut self, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        let mut out = Vec::with_capacity(len as usize);
        let Some(indices) = self.indices(offset, len)? else { return Ok(out) };
        if !indices.clone().all(|i| self.held.contains_key(&i)) {
            self.hold(std::slice::from_ref(&indices))?;
        }
        let end = offset + len;
        for (index, data) in self.held.range(indices) {
            let start = self.starts[*index];
            let from = offset.saturating_sub(start) as usize;
            let to = ((end - start) as usize).min(data.len());
            out.extend_from_slice(&data[from..to]);
        }
        Ok(out)
    }

    /// Holds the chunks that `ranges`, each `(offset, len)`, overlap, and no others: the ones not
    /// held yet in one request. Reads inside the ranges then fetch nothing.
    pub fn preload(&mut self, ranges: &[(u64, u64)]) -> Result<(), Error> {
        let mut spans = Vec::with_capacity(ranges.len());
        for &(offset, len) in ranges {
            spans.extend(self.indices(offset, len)?);
        }
        self.hold(&spans)
    }

    /// The chunks `[offset, offset + len)` overlaps, by index; none for an empty range.
    fn indices(&self, offset: u64, len: u64) -> Result<Option<Range<usize>>, Error> {
        let size = self.size();
        let end = offset.checked_add(len).filter(|&end| end <= size).ok_or(Error::Range { offset, len, size })?;
        if len == 0 {
            return Ok(None);
        }
        let first = self.starts.partition_point(|&s| s <= offset) - 1;
        let last = self.starts.partition_point(|&s| s < end);
        Ok(Some(first..last))
    }

    /// Makes `held` the chunks of `spans`, fetching the ones not held yet in one request.
    fn hold(&mut self, spans: &[Range<usize>]) -> Result<(), Error> {
        let wanted: BTreeSet<usize> = spans.iter().flat_map(Range::clone).collect();
        let mut previous = std::mem::take(&mut self.held);
        let missing: Vec<usize> = wanted.iter().copied().filter(|i| !previous.contains_key(i)).collect();
        let refs: Vec<ChunkRef> = missing.iter().map(|&i| self.chunks[i]).collect();
        let mut fetched: BTreeMap<usize, Vec<u8>> = missing.into_iter().zip(self.source.chunks(&refs)?).collect();
        for index in wanted {
            let data = previous.remove(&index).or_else(|| fetched.remove(&index)).expect("a chunk not held was fetched");
            self.held.insert(index, data);
        }
        Ok(())
    }
}

impl<S: ChunkSource + ?Sized> Read for FileReader<'_, S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = (buf.len() as u64).min(self.size().saturating_sub(self.pos));
        if len == 0 {
            return Ok(0);
        }
        let data = self.read_range(self.pos, len).map_err(io::Error::other)?;
        buf[..data.len()].copy_from_slice(&data);
        self.pos += len;
        Ok(data.len())
    }
}

impl<S: ChunkSource + ?Sized> Seek for FileReader<'_, S> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(n) => self.size().checked_add_signed(n),
            SeekFrom::Current(n) => self.pos.checked_add_signed(n),
        };
        self.pos = pos.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a seek before the start"))?;
        Ok(self.pos)
    }
}
