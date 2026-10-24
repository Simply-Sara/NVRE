//! The game's files and the schema that describes them.
//!
//! This is what a model, a texture or a piece of geometry is read through:
//! the virtual file system that knows where a file lives, and the schema
//! that says how a block is laid out.

use bevy::prelude::*;
use nvre_nif::Schema;
use nvre_vfs::Vfs;
use std::path::Path;
use std::sync::Arc;

/// The game's files and the schema that describes them.
#[derive(Resource, Clone)]
pub struct GameData {
    /// The virtual file system: the archives and the loose files.
    pub vfs: Arc<Vfs>,
    /// The schema, for reading a NIF's blocks.
    pub schema: Arc<Schema>,
}

impl GameData {
    /// Open the default archive set of `data_dir`, and load the schema.
    ///
    /// `schema_dir` is the repository's `schemas/nif` directory: the base
    /// schema, and the layers over it, are read from it.
    pub fn open(data_dir: &Path, schema_dir: &Path) -> Result<GameData, String> {
        let vfs = Vfs::open_default(data_dir).map_err(|e| e.to_string())?;
        let schema = load_schema(schema_dir, &[])?;
        Ok(GameData {
            vfs: Arc::new(vfs),
            schema: Arc::new(schema),
        })
    }

    /// [`GameData::open`] with the archives of `plugins` — the load order, so
    /// a mod's archive is read over the base game's.
    pub fn open_for_plugins<S: AsRef<str>>(
        data_dir: &Path,
        plugins: &[S],
        schema_dir: &Path,
    ) -> Result<GameData, String> {
        let vfs = Vfs::open_for_plugins(data_dir, plugins).map_err(|e| e.to_string())?;
        let schema = load_schema(schema_dir, &[])?;
        Ok(GameData {
            vfs: Arc::new(vfs),
            schema: Arc::new(schema),
        })
    }

    /// [`GameData::open`] with explicit archives and schema layers.
    pub fn open_with_archives<S: AsRef<str>>(
        data_dir: &Path,
        archives: &[S],
        schema_dir: &Path,
        layers: &[&str],
    ) -> Result<GameData, String> {
        let arch_strings: Vec<String> = archives.iter().map(|s| s.as_ref().to_string()).collect();
        let vfs = Vfs::open(data_dir, &arch_strings).map_err(|e| e.to_string())?;
        let schema = load_schema(schema_dir, layers)?;
        Ok(GameData {
            vfs: Arc::new(vfs),
            schema: Arc::new(schema),
        })
    }

    /// [`GameData::open`] with the schema's layers over the base.
    ///
    /// The layers are named directories under `schema_dir`, in the order they
    /// are given: the family's first, then the version's.
    pub fn open_with_schema_layers(
        data_dir: &Path,
        schema_dir: &Path,
        layers: &[&str],
    ) -> Result<GameData, String> {
        let vfs = Vfs::open_default(data_dir).map_err(|e| e.to_string())?;
        let schema = load_schema(schema_dir, layers)?;
        Ok(GameData {
            vfs: Arc::new(vfs),
            schema: Arc::new(schema),
        })
    }

    /// Read a file, or `None` when it is not in any archive.
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self.vfs.read(path)? {
            Ok(d) => Some(d),
            Err(e) => {
                warn!("{path}: {e}");
                None
            }
        }
    }
}

/// Read the NIF schema: the base, and any layers over it.
///
/// The base lives in `base` under the schema directory; a layer is a
/// directory of the same name as the layer — `skyrim`, `skyrimse` — and is
/// read over the base.
fn load_schema(schema_dir: &Path, layers: &[&str]) -> Result<Schema, String> {
    let base = schema_dir.join("base");
    if !base.is_dir() {
        return Err(format!("{}: the base schema is not there", base.display()));
    }
    let mut chain: Vec<String> = Vec::new();
    for l in layers {
        let dir = schema_dir.join(l);
        if dir.is_dir() {
            chain.push(dir.to_string_lossy().into_owned());
        }
    }
    Schema::load_game(&base, &chain).map_err(|e| e.to_string())
}

/// A plugin that puts the game's files in the world.
pub struct ArchivePlugin(pub GameData);

impl Plugin for ArchivePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::nif_material::NifMaterialPlugin)
            .insert_resource(self.0.clone())
            .init_resource::<crate::texture::TextureCache>()
            .init_resource::<crate::model::ModelCache>();
    }
}
