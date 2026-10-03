//! What the tip holds (`docs/APPEND.md`, "What the tip says"): its files by WAD directory and by
//! entry, borrowed from one `ls-tree` listing, which at 1.4 million files is worth not copying;
//! and the few facts read back from them.

use std::collections::HashMap;

use sync_format::{BuildFacts, Rads};

use crate::Error;

/// What the tip holds of one WAD.
#[derive(Debug, Default)]
pub struct TipWad<'a> {
    /// `_wad.yaml`'s blob.
    pub wad_yaml: Option<&'a str>,
    /// Each entry's files, by path hash: its own file and a bin's entries, (path, blob).
    pub entries: HashMap<u64, Vec<(&'a str, &'a str)>>,
}

/// The tip's WADs, by directory.
#[derive(Debug, Default)]
pub struct Tip<'a> {
    pub wads: HashMap<&'a str, TipWad<'a>>,
}

impl<'a> Tip<'a> {
    /// The index of an `ls-tree -r -z` listing.
    pub fn index(listing: &'a str) -> Result<Self, Error> {
        let mut tip = Tip::default();
        for record in listing.split('\0').filter(|r| !r.is_empty()) {
            let (meta, path) = record.split_once('\t').ok_or_else(|| Error::Tip(format!("an ls-tree record {record:?}")))?;
            let blob = meta.rsplit(' ').next().expect("rsplit yields one item");
            if !path.starts_with("files/") {
                continue;
            }
            if let Some(dir) = path.strip_suffix("/_wad.yaml") {
                tip.wads.entry(dir).or_default().wad_yaml = Some(blob);
            } else if let Some((dir, hash)) = entry_of(path) {
                tip.wads.entry(dir).or_default().entries.entry(hash).or_default().push((path, blob));
            } else {
                return Err(Error::Tip(format!("{path} fits no file of the layout")));
            }
        }
        Ok(tip)
    }
}

/// The WAD directory and path hash of the entry a file belongs to: its own
/// `<dir>/xx/<path hash>.yaml`, or one of a bin's `<dir>/xx/<path hash>.bin/yy/<entry>.rito`.
fn entry_of(path: &str) -> Option<(&str, u64)> {
    let own = match path.rsplitn(3, '/').collect::<Vec<_>>()[..] {
        [file, _, bin] if file.ends_with(".rito") && bin.ends_with(".bin") => bin,
        _ => path,
    };
    let [name, _, dir] = own.rsplitn(3, '/').collect::<Vec<_>>()[..] else { return None };
    let hash = name.strip_suffix(".yaml").or_else(|| name.strip_suffix(".bin"))?;
    (hash.len() == 16).then_some(())?;
    Some((dir, u64::from_str_radix(hash, 16).ok()?))
}

/// Whether a file is an entry's own, `xx/<path hash>.yaml`, not one of a bin's entries.
pub fn is_own_file(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(|name| name.len() == 21 && name.ends_with(".yaml"))
}

/// A top-level field's value, unquoted.
fn field<'t>(text: &'t str, key: &str) -> Option<&'t str> {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(": ")).map(|v| v.trim_matches('"'))
}

/// What `_wad.yaml` says the WAD was: its file id and its tags.
pub fn wad_ids(text: &str) -> (Option<u64>, Vec<String>) {
    let file_id = field(text, "fileId").and_then(|v| u64::from_str_radix(v, 16).ok());
    let tags = text.lines().filter_map(|l| l.strip_prefix(" - ")).map(|t| t.trim_matches('"').to_string()).collect();
    (file_id, tags)
}

/// The checksum an entry's own file holds.
pub fn checksum(text: &str) -> Option<u64> {
    field(text, "checksum").and_then(|v| u64::from_str_radix(v, 16).ok())
}

/// What a `build.yaml` says.
pub fn build_facts(text: &str) -> Result<BuildFacts, Error> {
    let get = |key: &str| field(text, key).map(str::to_string).ok_or_else(|| Error::Tip(format!("build.yaml without {key}")));
    Ok(BuildFacts {
        version: get("version")?,
        patch: get("patch")?,
        manifest: u64::from_str_radix(&get("manifest")?, 16).map_err(|e| Error::Tip(format!("build.yaml's manifest: {e}")))?,
        rads: match get("source")?.as_str() {
            "rads" => Some(Rads { solution: get("solution")?, release: get("release")?, exe: get("exe")? }),
            _ => None,
        },
        date: get("date")?,
        legacy_bins: field(text, "legacyBins") == Some("true"),
        realms: text.lines().filter_map(|l| l.strip_prefix(" - ")).map(|r| r.trim_matches('"').to_string()).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_belong_to_their_wad_and_entry() {
        let dir = "files/data/final/a.wad.client";
        assert_eq!(entry_of(&format!("{dir}/22/22e2cf785baeac7e.yaml")), Some((dir, 0x22e2_cf78_5bae_ac7e)));
        assert_eq!(entry_of(&format!("{dir}/22/22e2cf785baeac7e.bin/b0/b08673e8.rito")), Some((dir, 0x22e2_cf78_5bae_ac7e)));
        assert_eq!(entry_of(&format!("{dir}/22/22e2.yaml")), None);
        assert!(is_own_file(&format!("{dir}/22/22e2cf785baeac7e.yaml")));
        assert!(!is_own_file(&format!("{dir}/22/22e2cf785baeac7e.bin/b0/b08673e8.rito")));
    }
}
