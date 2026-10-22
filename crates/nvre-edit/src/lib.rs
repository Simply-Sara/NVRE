//! `nvre-edit` — the write path for TES4 plugins.
//!
//! NVRE's own `nvre-esm` reads plugins losslessly and writes them back byte
//! for byte, but it has no way to *change* one. This crate adds that:
//!
//! - [`encode`]: JSON values back into subrecord payloads, the exact inverse of
//!   `nvre_esm::decode`. Anything that does not round-trip is refused.
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

pub mod cell;
pub mod document;
pub mod encode;
pub mod formid;
pub mod save;

pub use cell::{NewCell, NewWorldspace};
pub use document::{Document, Edit, PendingRecord, RecordRow};
pub use formid::{count_formids, formid_fields, remap_sub};
pub use save::{
    empty_header, formid_map_for, write_plugin, write_plugin_scoped, write_plugin_with_load_order,
    MasterList, SaveReport,
};
