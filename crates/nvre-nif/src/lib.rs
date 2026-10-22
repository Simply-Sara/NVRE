//! NVRE's NIF reader.
//!
//! A NIF is a list of blocks, and each block's fields are laid out in an
//! order that depends on its type and on the file's version. The reader is
//! driven by a **schema** — a description of those layouts, loaded from JSON
//! at runtime — so a wrong layout is fixed by editing data, not by editing
//! code, and so several games' layouts can live side by side.
//!
//! # The pieces
//!
//! - [`schema`] — the layouts: what a block's fields are, and when.
//! - [`expr`] — the expression language a schema's conditions and lengths
//!   are written in.
//! - [`codec`] — reading bytes into values.
//! - [`value`] — what a decoded block holds.
//! - [`file`] — the file itself: its header, its string table and its
//!   blocks.
//! - [`scene`] — the scene graph: the nodes, the shapes, and the geometry.
//! - [`anim`] — the animation sequences.
//!
//! # Layers
//!
//! A schema is loaded from a base and any number of layers over it. A layer
//! may replace a block or a struct outright, or add to one. That is how a
//! game's own layouts sit on top of a shared base: the base holds what every
//! game has, and a game's layer holds what is different about it.

pub mod anim;
pub mod codec;
pub mod encode;
pub mod expr;
pub mod file;
pub mod header;
pub mod scene;
pub mod schema;
pub mod value;

pub use file::{BlockStatus, NifFile, RawBlock};
pub use header::{Header, NifError};
pub use schema::Schema;
pub use value::{Struct, Value};
