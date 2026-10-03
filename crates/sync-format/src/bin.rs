//! A `PROP` bin split into its links and objects by the objects' size prefixes, without decoding a
//! property, and each object's keys and ritobin text (`docs/FORMAT.md`, "objects").

use std::collections::HashSet;
use std::ops::Range;

use sha2::{Digest, Sha256};

use crate::object::render_object;
use crate::read::{u16_at, u32_at};
use crate::yaml::ObjectKeys;
use crate::Error;

/// A `PROP` bin's parts, in stored order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinSplit {
    /// The bin format's version; links are stored from version 2.
    pub version: u32,
    /// The paths of the bins this one depends on.
    pub links: Vec<String>,
    /// Every object, repeated entry hashes included.
    pub objects: Vec<ObjectSpan>,
}

/// One object of a `PROP` bin: its entry hash, its class from the bin's class table, and where its
/// bytes are, from the entry hash to the end of its properties.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSpan {
    /// FNV-1a 32 of the object's name.
    pub entry_hash: u32,
    /// FNV-1a 32 of the object's class name.
    pub class: u32,
    /// Where the object's bytes are in the bin's.
    pub range: Range<usize>,
}

/// A bin's facts: its links, its objects' keys, and the ritobin text of each object that parses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinFacts {
    /// In stored order.
    pub links: Vec<String>,
    /// The first object of each entry hash, in stored order.
    pub objects: Vec<ObjectKeys>,
    /// Each object's text, by entry hash; an object that does not parse has none.
    pub texts: Vec<(u32, String)>,
    /// Objects that did not parse.
    pub unrenderable: usize,
    /// Objects left out because an earlier object of the bin has their entry hash.
    pub repeated: usize,
}

/// Splits a `PROP` bin. Independent of the property numbering and of any schema; a `PTCH` bin is
/// not one.
pub fn split_bin(bytes: &[u8]) -> Result<BinSplit, Error> {
    let fail = |what: &str| Error::Bin(what.to_string());
    if bytes.get(0..4) != Some(b"PROP") {
        return Err(fail("not a PROP bin"));
    }
    let version = u32_at(bytes, 4).ok_or_else(|| fail("truncated at the version"))?;
    let mut pos = 8;
    let mut links = Vec::new();
    if version >= 2 {
        let count = u32_at(bytes, pos).ok_or_else(|| fail("truncated at the link count"))?;
        pos += 4;
        for _ in 0..count {
            let len = u16_at(bytes, pos).ok_or_else(|| fail("truncated in the links"))? as usize;
            pos += 2;
            let text = bytes.get(pos..pos + len).ok_or_else(|| fail("truncated in a link"))?;
            links.push(String::from_utf8_lossy(text).into_owned());
            pos += len;
        }
    }
    let count = u32_at(bytes, pos).ok_or_else(|| fail("truncated at the object count"))? as usize;
    pos += 4;
    let classes = count
        .checked_mul(4)
        .and_then(|n| bytes.get(pos..pos.checked_add(n)?))
        .ok_or_else(|| fail("truncated in the class table"))?;
    pos += classes.len();
    let mut objects = Vec::with_capacity(count);
    for &class in classes.as_chunks::<4>().0 {
        let class = u32::from_le_bytes(class);
        let size = u32_at(bytes, pos).ok_or_else(|| fail("truncated at an object size"))? as usize;
        pos += 4;
        let end = pos.checked_add(size).filter(|&end| end <= bytes.len()).ok_or_else(|| fail("an object runs past the end"))?;
        let entry_hash = u32_at(bytes, pos).ok_or_else(|| fail("an object shorter than its entry hash"))?;
        objects.push(ObjectSpan { entry_hash, class, range: pos..end });
        pos = end;
    }
    if pos != bytes.len() {
        return Err(Error::Bin(format!("{} bytes after the last object", bytes.len() - pos)));
    }
    Ok(BinSplit { version, links, objects })
}

/// A bin's facts from its bytes. An entry hash the bin repeats keeps its first object, both in
/// the keys and in the texts.
pub fn bin_facts(bytes: &[u8], legacy: bool) -> Result<BinFacts, Error> {
    let split = split_bin(bytes)?;
    let mut seen = HashSet::new();
    let (mut objects, mut texts, mut unrenderable) = (Vec::new(), Vec::new(), 0);
    for span in split.objects.iter().filter(|o| seen.insert(o.entry_hash)) {
        let data = &bytes[span.range.clone()];
        // An object's size can be under the four bytes of the entry hash `split_bin` reads.
        let norm = Sha256::digest(data.get(4..).unwrap_or_default());
        objects.push(ObjectKeys {
            entry_hash: span.entry_hash,
            class: span.class,
            content: Sha256::digest(data).into(),
            norm: norm[..8].try_into().expect("a SHA-256 has eight bytes"),
        });
        match render_object(data, span.class, legacy) {
            Ok(text) => texts.push((span.entry_hash, text)),
            Err(_) => unrenderable += 1,
        }
    }
    let repeated = split.objects.len() - objects.len();
    Ok(BinFacts { links: split.links, objects, texts, unrenderable, repeated })
}
