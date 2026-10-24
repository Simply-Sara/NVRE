//! NVRE's record reader.
//!
//! A TES4-family plugin (an `.esm` or `.esp`) is a tree of **groups**, each
//! holding **records**, each holding **subrecords**. This crate reads that
//! structure, and reads what the records *mean* from a schema.
//!
//! The layers:
//!
//! - [`raw`] — the container: groups, records, subrecords, and a writer
//!   that reproduces the file byte for byte.
//! - [`plugin`] — one file, with its TES4 header.
//! - [`loadorder`] — the masters, the load order, and the FormIDs that
//!   resolve to a particular plugin.
//! - [`schema`] — the record layouts, loaded from JSON at runtime.
//! - [`decode`] — decoding a record's fields against a schema, and counting
//!   how much of the data the schema explains.
//!
//! The container layer knows nothing about what a record means, and the
//! schema layer knows nothing about files. That split is what lets several
//! games' layouts sit side by side.

pub mod decode;
pub mod inventory;
pub mod loadorder;
pub mod plugin;
pub mod raw;
pub mod schema;
pub mod strings;
pub mod typedef;

pub use loadorder::LoadOrder;
pub use plugin::Plugin;
pub use raw::MasterRef;
pub use raw::PluginHeader;
pub use raw::{EsmError, Group, GroupHeader, Node, Record, RecordHeader, Subrecord};
pub use schema::Schema;
pub use typedef::{FieldDef, TypeDef};
