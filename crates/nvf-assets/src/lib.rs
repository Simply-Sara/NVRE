//! NVRE's Bevy asset layer.
//!
//! The pieces a game needs to draw itself:
//!
//! - [`data`] — the game's archives and the NIF schema, as resources.
//! - [`model`] — the model cache, and the model a NIF describes.
//! - [`shape`] — a shape's geometry, and the material it is drawn with.
//! - [`nif_material`] — the material: Bevy's `StandardMaterial` with the
//!   changes a NIF's shapes need.
//! - [`texture`] — the texture cache, and the DDS reader behind it.
//!
//! What is *not* here, and was in the layer this replaces, is everything the
//! game needs but an editor does not: playing a model's idle animation, its
//! door clips, its particle systems, and releasing the assets nothing uses.
//! An editor draws a model; it does not act it out.

pub mod data;
pub mod model;
pub mod nif_material;
pub mod shape;
pub mod texture;

pub use data::{ArchivePlugin, GameData};
pub use model::{ModelCache, ModelCtx, PreparedModel, PreparedShape};
pub use nif_material::{nif_material, NifMaterial, NifMaterialPlugin};
pub use texture::TextureCache;
