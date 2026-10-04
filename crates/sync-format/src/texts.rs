//! The files whose text never varies: kept beside this crate and embedded as they are.

/// `.gitattributes`, added by `history-v2`'s first commit: every file is LF and full of hashes, and a
/// checkout that rewrote line endings would corrupt them.
pub const GITATTRIBUTES: &str = "* -text\n";

/// `census.yaml`: the format, and the schema for each path pattern.
pub const CENSUS_YAML: &str = include_str!("../schema/census.yaml");

/// `schema/`: each schema's path in the tree and its text.
pub const SCHEMAS: [(&str, &str); 3] = [
    ("schema/build.schema.json", include_str!("../schema/build.schema.json")),
    ("schema/entry.schema.json", include_str!("../schema/entry.schema.json")),
    ("schema/wad.schema.json", include_str!("../schema/wad.schema.json")),
];

/// `README.md` on `main`.
pub const MAIN_README: &str = include_str!("../main/README.md");

/// `LICENSE` on `main`: the data in the public domain, without warranty.
pub const MAIN_LICENSE: &str = include_str!("../main/LICENSE");
