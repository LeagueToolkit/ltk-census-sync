//! An entry's files from its bytes: its own file, with the section its kind has when the bytes
//! parse, and a bin's entries.

use sha2::{Digest, Sha256};

use crate::bank::bank_facts;
use crate::bin::bin_facts;
use crate::names::{entry_name, object_name};
use crate::rig::{mesh_facts, skeleton_facts};
use crate::texture::texture_facts;
use crate::yaml::{entry_yaml, EntryFacts, Section};
use crate::Error;

/// The files of one WAD entry.
#[derive(Debug)]
pub struct EntryFiles {
    /// Each file's path in its WAD's directory, and its text.
    pub files: Vec<(String, String)>,
    /// Why the bytes gave no section, for a kind that has one.
    pub error: Option<Error>,
    /// Bin objects that did not parse: listed in `objects`, with no file.
    pub unrenderable: usize,
    /// Bin objects left out because an earlier object of the bin has their entry hash.
    pub repeated: usize,
}

/// An entry's files from its bytes (decompressed) and what the WAD's table says of it. `kind` is
/// `kind_of(bytes)`, or `link` for an entry the table stores as a link; `legacy` is the build's
/// property-kind numbering (`legacy_bins`).
pub fn entry_files(path_hash: u64, checksum: Option<u64>, kind: &str, bytes: &[u8], legacy: bool) -> EntryFiles {
    let name = entry_name(path_hash);
    let mut out = EntryFiles { files: Vec::new(), error: None, unrenderable: 0, repeated: 0 };
    let mut links = None;
    let section = match kind {
        // A `PTCH` bin (an override) does not split: no links, no objects, no entries.
        "bin" if bytes.starts_with(b"PTCH") => Ok(None),
        "bin" => bin_facts(bytes, legacy).map(|bin| {
            out.unrenderable = bin.unrenderable;
            out.repeated = bin.repeated;
            out.files.extend(bin.texts.into_iter().map(|(entry, text)| (format!("{name}.bin/{}", object_name(entry)), text)));
            links = Some(bin.links);
            Some(Section::Objects(bin.objects))
        }),
        "bnk" | "wpk" => bank_facts(bytes).map(|b| Some(Section::Bank(b))),
        "skl" => skeleton_facts(bytes).map(|s| Some(Section::Skeleton(s))),
        "skn" => mesh_facts(bytes).map(|m| Some(Section::Mesh(m))),
        "tex" | "dds" => texture_facts(bytes).map(|t| Some(Section::Texture(t))),
        _ => Ok(None),
    };
    let section = section.unwrap_or_else(|e| {
        out.error = Some(e);
        None
    });
    let facts = EntryFacts { sha256: Sha256::digest(bytes).into(), checksum, kind: kind.to_string(), links, section };
    out.files.push((format!("{name}.yaml"), entry_yaml(&facts)));
    out
}
