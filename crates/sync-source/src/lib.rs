//! Where a build's bytes come from (`docs/SOURCES.md`): the live builds to append, their RMAN
//! manifests, and chunks by id from a local cache or Riot's CDN, rebuilt into a file or a range of
//! one through `Read + Seek`.
//!
//! Shaped as a general manifest crate would be, so it can move into the LeagueToolkit family
//! whole: nothing here knows about the history.
