//! `nvre-edit` — the write path for NVForever's TES4 plugins.
//!
//! NVForever's own `fnv-esm` reads plugins losslessly and writes them back byte
//! for byte, but it has no way to *change* one. This crate adds that:
//!
//! - [`encode`]: JSON values back into subrecord payloads, the exact inverse of
//!   `fnv_esm::decode`. Anything that does not round-trip is refused.
//! - [`document`]: a mutable plugin — edit fields, insert and delete references,
//!   and save. Untouched records keep their original bytes.
//! - [`formid`]: rewriting the FormIDs a record carries when it moves into a
//!   different plugin, schema-driven so nothing is guessed at.
//! - [`save`]: writing the result as a **new plugin** with its own master list,
//!   which is what makes the output loadable as a mod rather than a copy of a
//!   master.
//!
//! All of it is format-level and headless, so it can be tested against the
//! shipped masters without a window.

pub mod document;
pub mod encode;
pub mod formid;
pub mod save;

pub use document::{Document, Edit, PendingRecord, RecordRow};
pub use formid::{count_formids, formid_fields, remap_sub};
pub use save::{empty_header, formid_map_for, write_plugin, MasterList, SaveReport};
