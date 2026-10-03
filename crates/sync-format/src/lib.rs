//! Format 2 of the census history, as written: what each file is named, what it holds, and the
//! exact bytes it holds it in (`docs/FORMAT.md`).
//!
//! The format is frozen. Every writer here is a pure function of facts read from one WAD or one
//! WAD entry, and its output is compared byte for byte with the published history before a change
//! lands (`docs/TOOLING.md`, "The oracle"). The texts that never vary -- `census.yaml`, the
//! schemas, `main`'s README -- are kept as files beside this crate and embedded as they are.
