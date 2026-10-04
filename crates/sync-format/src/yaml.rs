//! The YAML families (`docs/FORMAT.md`, "The YAML"): `build.yaml`, `_wad.yaml`, and an entry's own
//! file with the section its kind has. Each renderer takes the facts a file says and returns its
//! exact text; nothing else writes them.
//!
//! A section is rendered at the top level and nested under its key by one space, a blank line
//! excepted, so each section's renderer reads as the text it writes.

use std::collections::HashMap;
use std::fmt::Write as _;

/// What `build.yaml` says: one build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildFacts {
    /// The client's version, `16.19.8207193`.
    pub version: String,
    /// Season and patch, `16.19`: the tag's name.
    pub patch: String,
    /// The RMAN manifest id; for a RADS build, the id the export gave it.
    pub manifest: u64,
    /// Where a build read from RADS came from; none for an RMAN build.
    pub rads: Option<Rads>,
    /// The day the manifest was published, `YYYY-MM-DD`, UTC.
    pub date: String,
    /// Whether the build's bins use the property-kind numbering of builds before 10.8.
    pub legacy_bins: bool,
    /// The live realms that shipped the build, in any order.
    pub realms: Vec<String>,
}

/// The RADS release a build of 8.20 to 9.1 was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rads {
    /// The solution's release, `0.0.1.243`.
    pub solution: String,
    /// The game client project's release in it, `0.0.1.184`.
    pub release: String,
    /// The client executable's file version, `8.20.248.3196`.
    pub exe: String,
}

impl BuildFacts {
    /// `rman` or `rads`: which kind of id `manifest` is.
    pub fn source(&self) -> &'static str {
        if self.rads.is_some() { "rads" } else { "rman" }
    }

    /// The realms, sorted and deduplicated, as `build.yaml` and the commit's trailer list them.
    pub fn sorted_realms(&self) -> Vec<&str> {
        let mut realms: Vec<&str> = self.realms.iter().map(String::as_str).collect();
        realms.sort_unstable();
        realms.dedup();
        realms
    }
}

/// What an entry's own file, `xx/<path hash>.yaml`, says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryFacts {
    /// Of the entry's bytes, decompressed.
    pub sha256: [u8; 32],
    /// The checksum the WAD's table holds for the entry; none in a WAD without them.
    pub checksum: Option<u64>,
    /// What the bytes are by their magic (`kind_of`), or `link`; empty when unknown.
    pub kind: String,
    /// A bin's links, in any order, for a bin that splits; none for every other entry.
    pub links: Option<Vec<String>>,
    /// What the bytes say, when the kind has a section and the bytes parse.
    pub section: Option<Section>,
}

/// What an entry's bytes say, under one key for its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Section {
    /// `skeleton`, of a `skl`.
    Skeleton(SkeletonFacts),
    /// `mesh`, of a `skn`.
    Mesh(MeshFacts),
    /// `texture`, of a `tex` or `dds`.
    Texture(TextureFacts),
    /// `bank`, of a `bnk` or `wpk`.
    Bank(BankFacts),
    /// A bin's objects, in any order.
    Objects(Vec<ObjectKeys>),
    /// `clip`, of an `anm`.
    Clip(ClipFacts),
}

/// An animation clip (`anm`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipFacts {
    /// The hash of each joint the clip animates, in stored order.
    pub joints: Vec<u32>,
}

/// A skeleton (`skl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonFacts {
    /// The number of joints that influence vertices.
    pub influences: u64,
    /// Empty when the skeleton stores none.
    pub name: String,
    /// Empty when the skeleton stores none.
    pub asset: String,
    /// In stored order.
    pub joints: Vec<Joint>,
}

/// One joint of a skeleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Joint {
    /// As stored; another joint can have the same name.
    pub name: String,
    /// The parent's ordinal; none for a root.
    pub parent: Option<usize>,
}

/// A mesh (`skn`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshFacts {
    /// ltk_mesh's name for the vertex layout, `Basic`; empty when it has none.
    pub vertex_type: String,
    /// The vertex buffer's count.
    pub vertices: u64,
    /// The index buffer's count.
    pub indices: u64,
    /// In stored order.
    pub submeshes: Vec<SubmeshFacts>,
}

/// One submesh of a mesh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmeshFacts {
    /// Its material's name.
    pub name: String,
    /// The vertices of its range that the vertex buffer holds.
    pub vertices: u64,
    /// Its index count.
    pub indices: u64,
    /// XXH3 of the positions as stored.
    pub positions: u64,
    /// XXH3 of the positions rounded to 1/16 unit and sorted.
    pub rounded: u64,
}

/// A texture (`tex`, `dds`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureFacts {
    /// ltk_texture's name for a `tex` format (`bc3`), ddsfile's for a `dds` one (`dxt5`), lowercase.
    pub format: String,
    /// Of the top mip, in pixels.
    pub width: u64,
    /// Of the top mip, in pixels.
    pub height: u64,
    /// The number of mips, the top one included.
    pub mips: u64,
    /// XXH3 of the top mip's stored bytes.
    pub top: u64,
}

/// A sound bank (`bnk`, `wpk`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankFacts {
    /// The Wwise bank version, or the wpk version.
    pub version: u64,
    /// The bank's id from its header; none for a wpk.
    pub bank_id: Option<u32>,
    /// One per wem id, in any order.
    pub media: Vec<MediaFacts>,
    /// Each event's id and the wem ids its play actions reach, one per event id, in any order.
    pub events: Vec<(u32, Vec<u32>)>,
    /// The `BKHD` section's body, for a bank with a `HIRC` section; none for any other.
    pub header: Option<Vec<u8>>,
    /// Every `HIRC` object, in stored order.
    pub objects: Vec<BankObject>,
}

/// One object of a bank's hierarchy, as the bank stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankObject {
    /// The object's id; none for an object too short to hold one.
    pub id: Option<u32>,
    /// The object's type byte.
    pub kind: u8,
    /// The object's bytes after its id.
    pub body: Vec<u8>,
}

/// One wem a bank holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFacts {
    /// The wem's id.
    pub id: u32,
    /// The wem's size in bytes.
    pub size: u64,
    /// The first eight bytes of the SHA-256 of the wem's bytes.
    pub hash: [u8; 8],
}

/// One object of a bin, by its entry hash: keys over its binary bytes, which no reader of its
/// ritobin text can recover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectKeys {
    /// FNV-1a 32 of the object's name.
    pub entry_hash: u32,
    /// FNV-1a 32 of the object's class name.
    pub class: u32,
    /// SHA-256 of the object's bytes, entry hash to end.
    pub content: [u8; 32],
    /// The first eight bytes of the SHA-256 of the object's bytes after its entry hash.
    pub norm: [u8; 8],
}

/// `build.yaml`.
pub fn build_yaml(build: &BuildFacts) -> String {
    let mut text = String::new();
    field(&mut text, "version", &build.version);
    field(&mut text, "patch", &build.patch);
    let _ = writeln!(text, "manifest: \"{:016x}\"", build.manifest);
    field(&mut text, "source", build.source());
    if let Some(rads) = &build.rads {
        field(&mut text, "solution", &rads.solution);
        field(&mut text, "release", &rads.release);
        field(&mut text, "exe", &rads.exe);
    }
    field(&mut text, "date", &build.date);
    let _ = writeln!(text, "legacyBins: {}", build.legacy_bins);
    list(&mut text, "realms", build.sorted_realms());
    text
}

/// A manifest's tags for a file as `_wad.yaml` lists them: sorted and deduplicated, `none` dropped.
pub fn wad_tags<'a>(tags: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut tags: Vec<String> = tags.into_iter().filter(|t| !t.is_empty() && *t != "none").map(str::to_string).collect();
    tags.sort_unstable();
    tags.dedup();
    tags
}

/// A WAD's `_wad.yaml`: the WAD format's version, the manifest's file id, and the manifest's tags
/// for the file.
pub fn wad_yaml<'a>(major: u8, minor: u8, file_id: u64, tags: impl IntoIterator<Item = &'a str>) -> String {
    let mut text = format!("version: \"{major}.{minor}\"\nfileId: \"{file_id:016x}\"\n");
    list(&mut text, "tags", wad_tags(tags).iter().map(String::as_str));
    text
}

/// An entry's own file, `xx/<path hash>.yaml`.
pub fn entry_yaml(entry: &EntryFacts) -> String {
    let mut text = String::from("sha256: ");
    hex(&mut text, &entry.sha256);
    text.push('\n');
    if let Some(checksum) = entry.checksum {
        let _ = writeln!(text, "checksum: \"{checksum:016x}\"");
    }
    if !entry.kind.is_empty() {
        field(&mut text, "kind", &entry.kind);
    }
    if let Some(links) = &entry.links {
        let mut links: Vec<&str> = links.iter().map(String::as_str).collect();
        links.sort_unstable();
        links.dedup();
        list(&mut text, "links", links);
    }
    if let Some(section) = &entry.section {
        let (key, body) = match section {
            Section::Skeleton(s) => ("skeleton", skeleton_body(s)),
            Section::Mesh(m) => ("mesh", mesh_body(m)),
            Section::Texture(t) => ("texture", texture_body(t)),
            Section::Bank(b) => ("bank", bank_body(b)),
            Section::Objects(o) => ("objects", objects_body(o)),
            Section::Clip(c) => ("clip", clip_body(c)),
        };
        // A section with nothing in it is not written: a bin with no objects has no `objects`.
        if !body.is_empty() {
            let _ = writeln!(text, "{key}:");
            for line in body.split_inclusive('\n') {
                if line != "\n" {
                    text.push(' ');
                }
                text.push_str(line);
            }
        }
    }
    text
}

fn clip_body(clip: &ClipFacts) -> String {
    let joints: Vec<String> = clip.joints.iter().map(|j| format!("{j:08x}")).collect();
    let mut text = String::new();
    list(&mut text, "joints", joints.iter().map(String::as_str));
    text
}

fn objects_body(objects: &[ObjectKeys]) -> String {
    let mut sorted: Vec<&ObjectKeys> = objects.iter().collect();
    sorted.sort_by_key(|o| o.entry_hash);
    let mut text = String::with_capacity(objects.len() * 110);
    for o in sorted {
        let _ = writeln!(text, "\"{:08x}\":\n class: \"{:08x}\"", o.entry_hash, o.class);
        text.push_str(" content: ");
        hex(&mut text, &o.content);
        text.push_str("\n norm: ");
        hex(&mut text, &o.norm);
        text.push('\n');
    }
    text
}

fn bank_body(bank: &BankFacts) -> String {
    let mut text = format!("version: {}\n", bank.version);
    if let Some(id) = bank.bank_id {
        let _ = writeln!(text, "bankId: \"{id:08x}\"");
    }
    if let Some(header) = &bank.header {
        text.push_str("header: ");
        hex(&mut text, header);
        text.push('\n');
    }
    if !bank.media.is_empty() {
        let mut media: Vec<&MediaFacts> = bank.media.iter().collect();
        media.sort_by_key(|m| m.id);
        text.push_str("media:\n");
        for m in media {
            let _ = writeln!(text, " \"{:08x}\":\n  size: {}", m.id, m.size);
            text.push_str("  hash: ");
            hex(&mut text, &m.hash);
            text.push_str("\n\n");
        }
    }
    if !bank.events.is_empty() {
        let mut events: Vec<&(u32, Vec<u32>)> = bank.events.iter().collect();
        events.sort_unstable();
        text.push_str("events:\n");
        for (id, wems) in events {
            if wems.is_empty() {
                let _ = writeln!(text, " \"{id:08x}\": []");
            } else {
                let _ = writeln!(text, " \"{id:08x}\":");
                for wem in wems {
                    let _ = writeln!(text, "  - \"{wem:08x}\"");
                }
            }
            text.push('\n');
        }
    }
    if !bank.objects.is_empty() {
        text.push_str("objects:\n");
        for o in &bank.objects {
            match o.id {
                Some(id) => {
                    let _ = writeln!(text, " - id: \"{id:08x}\"\n   type: {}", o.kind);
                }
                None => {
                    let _ = writeln!(text, " - type: {}", o.kind);
                }
            }
            text.push_str("   body: ");
            hex(&mut text, &o.body);
            text.push('\n');
        }
    }
    text
}

fn skeleton_body(skeleton: &SkeletonFacts) -> String {
    let mut text = format!("influences: {}\n", skeleton.influences);
    if !skeleton.name.is_empty() {
        field(&mut text, "name", &skeleton.name);
    }
    if !skeleton.asset.is_empty() {
        field(&mut text, "asset", &skeleton.asset);
    }
    let mut named: HashMap<&str, usize> = HashMap::new();
    for joint in &skeleton.joints {
        *named.entry(joint.name.as_str()).or_default() += 1;
    }
    text.push_str(if skeleton.joints.is_empty() { "joints: []\n" } else { "joints:\n" });
    for joint in &skeleton.joints {
        text.push_str(" - name: ");
        quote(&mut text, &joint.name);
        text.push('\n');
        // A parent past the last joint is written as no parent, as the export did.
        let Some((ordinal, parent)) = joint.parent.and_then(|p| skeleton.joints.get(p).map(|j| (p, j))) else {
            continue;
        };
        if named[parent.name.as_str()] == 1 {
            text.push_str("   parent: ");
            quote(&mut text, &parent.name);
            text.push('\n');
        } else {
            let _ = writeln!(text, "   parent: {ordinal}");
        }
    }
    text
}

fn mesh_body(mesh: &MeshFacts) -> String {
    let mut text = String::new();
    field(&mut text, "vertexType", &mesh.vertex_type);
    let _ = writeln!(text, "vertices: {}\nindices: {}", mesh.vertices, mesh.indices);
    text.push_str(if mesh.submeshes.is_empty() { "submeshes: []\n" } else { "submeshes:\n" });
    for s in &mesh.submeshes {
        text.push_str(" - name: ");
        quote(&mut text, &s.name);
        let _ = writeln!(
            text,
            "\n   vertices: {}\n   indices: {}\n   positions: \"{:016x}\"\n   rounded: \"{:016x}\"",
            s.vertices, s.indices, s.positions, s.rounded
        );
    }
    text
}

fn texture_body(texture: &TextureFacts) -> String {
    let mut text = String::new();
    field(&mut text, "format", &texture.format);
    let _ = writeln!(
        text,
        "width: {}\nheight: {}\nmips: {}\ntop: \"{:016x}\"",
        texture.width, texture.height, texture.mips, texture.top
    );
    text
}

/// `key: "value"` and a newline.
fn field(text: &mut String, key: &str, value: &str) {
    text.push_str(key);
    text.push_str(": ");
    quote(text, value);
    text.push('\n');
}

/// A block list of quoted strings under `key`, `[]` when empty.
fn list<'a>(text: &mut String, key: &str, items: impl IntoIterator<Item = &'a str>) {
    let mut items = items.into_iter().peekable();
    text.push_str(key);
    if items.peek().is_none() {
        text.push_str(": []\n");
        return;
    }
    text.push_str(":\n");
    for item in items {
        text.push_str(" - ");
        quote(text, item);
        text.push('\n');
    }
}

/// A double-quoted YAML scalar: `"` and `\` escaped, a control character as `\xHH`.
fn quote(text: &mut String, s: &str) {
    text.push('"');
    for c in s.chars() {
        match c {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(text, "\\x{:02x}", c as u32);
            }
            c => text.push(c),
        }
    }
    text.push('"');
}

/// Bytes as quoted lowercase hex, in their order.
fn hex(text: &mut String, bytes: &[u8]) {
    text.push('"');
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text.push('"');
}

#[cfg(test)]
mod tests {
    use super::quote;

    #[test]
    fn quotes_what_yaml_would_retype_or_break() {
        let quoted = |s: &str| {
            let mut text = String::new();
            quote(&mut text, s);
            text
        };
        assert_eq!(quoted("8.20"), "\"8.20\"");
        assert_eq!(quoted("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(quoted("tab\there\u{7f}"), "\"tab\\x09here\\x7f\"");
    }
}
