//! Format 2 of the census history, as written: what each file is named, what it holds, and the
//! exact bytes it holds it in (`docs/FORMAT.md`).
//!
//! The format is frozen. Every writer here is a pure function of facts read from one WAD or one
//! WAD entry, and its output is compared byte for byte with the published history before a change
//! lands (`docs/TOOLING.md`, "The oracle"). The texts that never vary -- `census.yaml`, the
//! schemas, `main`'s README -- are kept as files beside this crate and embedded as they are.
//!
//! Two layers. The facts (`EntryFacts` and the structs it holds) are what a file says, read from an
//! entry's bytes by `bin_facts`, `bank_facts` and the others; the renderers (`build_yaml`,
//! `entry_yaml`, ...) turn facts into text and are the only code that does. `entry_files` does both
//! for one entry.

mod bank;
mod bin;
mod check;
mod commit;
mod entry;
mod error;
mod kind;
mod names;
mod object;
mod read;
mod rig;
mod texts;
mod texture;
mod yaml;

pub use bank::bank_facts;
pub use bin::{bin_facts, split_bin, BinFacts, BinSplit, ObjectSpan};
pub use check::Census;
pub use commit::{commit_message, commit_time, AUTHOR};
pub use entry::{entry_files, EntryFiles};
pub use error::Error;
pub use kind::{kind_of, KIND_LINK};
pub use names::{entry_name, object_name, wad_dir};
pub use object::{legacy_bins, parse_object, render_object};
pub use rig::{mesh_facts, skeleton_facts};
pub use texts::{CENSUS_YAML, GITATTRIBUTES, MAIN_README, SCHEMAS};
pub use texture::texture_facts;
pub use yaml::{
    build_yaml, entry_yaml, wad_tags, wad_yaml, BankFacts, BuildFacts, EntryFacts, Joint, MediaFacts, MeshFacts,
    ObjectKeys, Rads, Section, SkeletonFacts, SubmeshFacts, TextureFacts,
};
