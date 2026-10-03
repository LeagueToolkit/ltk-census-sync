//! A manifest file read as `Read + Seek` over its chunks (`docs/SOURCES.md`, "Files and ranges"):
//! a seek costs nothing, and a read fetches the chunks it overlaps, in one request to the source.

use std::io::{self, Read, Seek, SeekFrom};

use crate::chunk::{open_frame, ChunkSource};
use crate::rman::ChunkRef;
use crate::Error;

/// One file's bytes, from its chunks.
pub struct FileReader<'a, S: ChunkSource + ?Sized> {
    source: &'a S,
    chunks: Vec<ChunkRef>,
    /// Where each chunk starts; the last value is the file's size.
    starts: Vec<u64>,
    pos: u64,
    /// The chunks of the last read, by index: a WAD's entries lie in table order, and one chunk
    /// often ends one entry and starts the next.
    held: Vec<(usize, Vec<u8>)>,
    dec: zstd::bulk::Decompressor<'static>,
}

impl<'a, S: ChunkSource + ?Sized> FileReader<'a, S> {
    /// A file made of these chunks, in file order.
    pub fn new(source: &'a S, chunks: Vec<ChunkRef>) -> Result<Self, Error> {
        let mut starts = Vec::with_capacity(chunks.len() + 1);
        starts.push(0);
        for chunk in &chunks {
            starts.push(starts[starts.len() - 1] + u64::from(chunk.place.uncompressed_size));
        }
        Ok(Self { source, chunks, starts, pos: 0, held: Vec::new(), dec: zstd::bulk::Decompressor::new()? })
    }

    /// The file's size.
    pub fn size(&self) -> u64 {
        self.starts[self.starts.len() - 1]
    }

    /// The bytes at `[offset, offset + len)`.
    pub fn read_range(&mut self, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        let size = self.size();
        let end = offset.checked_add(len).filter(|&end| end <= size).ok_or(Error::Range { offset, len, size })?;
        let mut out = Vec::with_capacity(len as usize);
        if len == 0 {
            return Ok(out);
        }
        let first = self.starts.partition_point(|&s| s <= offset) - 1;
        let last = self.starts.partition_point(|&s| s < end);
        self.hold(first..last)?;
        for (index, data) in &self.held {
            let start = self.starts[*index];
            let from = offset.saturating_sub(start) as usize;
            let to = ((end - start) as usize).min(data.len());
            out.extend_from_slice(&data[from..to]);
        }
        Ok(out)
    }

    /// Makes `held` the chunks `range`, fetching the ones not held yet in one request.
    fn hold(&mut self, range: std::ops::Range<usize>) -> Result<(), Error> {
        let mut previous = std::mem::take(&mut self.held);
        let missing: Vec<usize> = range.clone().filter(|i| !previous.iter().any(|(j, _)| j == i)).collect();
        let wanted: Vec<ChunkRef> = missing.iter().map(|&i| self.chunks[i]).collect();
        let frames = self.source.frames(&wanted)?;
        let mut fetched = Vec::with_capacity(missing.len());
        for ((&index, chunk), frame) in missing.iter().zip(&wanted).zip(&frames) {
            fetched.push((index, open_frame(chunk, frame, &mut self.dec)?));
        }
        for index in range {
            let data = match previous.iter().position(|(j, _)| *j == index) {
                Some(at) => previous.swap_remove(at).1,
                None => {
                    let at = fetched.iter().position(|(j, _)| *j == index).expect("a missing chunk was fetched");
                    fetched.swap_remove(at).1
                }
            };
            self.held.push((index, data));
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
