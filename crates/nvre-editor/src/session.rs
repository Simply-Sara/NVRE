//! The editor's session state: what plugin is open, which cell is being edited,
//! and which reference is selected.
//!
//! This is deliberately plain data with no Bevy in it, so the rules about what
//! is selected and what has changed can be tested without a window. The
//! viewport reads it to decide what to draw; the panels read and write it.

use bevy::prelude::*;
use nvre_edit::Document;
use nvre_esm::{LoadOrder, Schema};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

/// The schema a session reads records with: `schema_dir`, overlaid with `game`'s
/// own differences when it has any.
///
/// A session is opened with a directory, because that is what a caller passes
/// on the command line or in a test, and with a game detected from the plugin's
/// folder. The two together decide the schema: every game reads
/// `schemas/base` plus its own directory, and FNV additionally carries the
/// Fallout directory because a second game's records may reference a type it
/// defines.
///
/// The chain is built by [`crate::game::GameProfile::load_schema_over`], which
/// owns the directory layout. Reading the base alone for a Fallout 3 session
/// would decode 3 subrecords as unknown and error on one more — the numbers
/// that led to the game directories existing.
///
/// The schemas themselves are compiled into the binary, so a built editor
/// needs no repository beside it. `schema_dir` names a directory of schema
/// files to read *instead of* the compiled-in base, which is how a developer
/// points the editor at a layout they are still writing. The game's own
/// layers are laid over it either way.
fn load_schema(
    schema_dir: &std::path::Path,
    game: &crate::game::GameProfile,
) -> Result<Schema, String> {
    // An empty path is not a directory: it means the compiled-in schemas.
    let base = (!schema_dir.as_os_str().is_empty()).then_some(schema_dir);
    game.load_schema_over(base)
}

type CachedRecordList = Arc<[(u32, String)]>;

/// The open editing session.
#[derive(Resource)]
pub struct Session {
    /// Which game's data this session is over. Drives the object-window group
    /// table, and is what a panel reads when it has to name the game.
    pub game: crate::game::GameProfile,
    /// The game's Data folder this session was opened in.
    ///
    /// A session knows one folder: the one its masters and its own file were
    /// read from. [`Session::open`] derives it from the plugin's parent, and
    /// [`Session::create`] is handed it. Keeping it on the session is what lets
    /// a save name a folder without either caller passing it.
    pub data_dir: std::path::PathBuf,
    /// The plugin being edited.
    pub doc: Document,
    /// The load order the plugin was opened in (masters first).
    pub plugins: &'static [nvre_esm::Plugin],
    /// The load order index, built once at startup.
    pub lo: LoadOrder<'static>,
    /// The ESM record schema.
    pub schema: Schema,
    /// The string tables of the load order's localized plugins, so a
    /// `FULL` that holds a 4-byte string ID resolves to the name the
    /// player sees rather than to decoded ID bytes.
    pub strings: nvre_esm::strings::StringTables,
    /// The world index: which reference is in which cell.
    pub index: nvre_world::WorldIndex,
    /// The masters the saved plugin will depend on.
    pub masters: Vec<String>,
    /// The plugin's author (`CNAM`) and description (`SNAM`), as the header
    /// carries them.
    ///
    /// Kept on the session because the save rewrites the header: a plugin
    /// created with an author must keep it, and one opened from disk must keep
    /// the author it already had rather than having it blanked.
    pub author: Option<String>,
    pub description: Option<String>,
    /// The cell currently open, by FormID.
    pub cell: Option<u32>,
    /// The selected reference, by FormID (0 for a record not yet saved).
    pub selected: Option<u32>,
    /// How many records this session has placed, for resolving a placeholder.
    placed_index: usize,
    /// The cached reference list, and the cell it was built for.
    refs: Vec<RefRow>,
    refs_cell: Option<u32>,
    /// Cached exterior grid references, and (cell, radius) they were built for.
    exterior_refs: Vec<RefRow>,
    exterior_refs_key: Option<(u32, i32)>,
    /// Base FormID → model path, resolved once per base object.
    models: HashMap<u32, Option<String>>,
    /// Base FormID → `LIGH` data, resolved once per base object.
    ///
    /// `None` means the base object is not a light, which is cached just like
    /// `Some`, so a non-light is walked only once per session too.
    lights: HashMap<u32, Option<LightData>>,
    /// The references table's resolved columns, and the cell they were built for.
    details: RefDetails,
    details_cell: Option<u32>,
    /// The place palette's query result, and the filter it was built for.
    ///
    /// Building it walks the load order, which is the editor's most expensive
    /// query; doing it every frame is what made the editor run at half a frame
    /// per second while a cell was open.
    palette_groups: Vec<ObjectGroup>,
    palette_filter: String,
    /// Which plugins the object window lists, one flag per plugin in load
    /// order, parallel to [`Session::lo`]'s plugins.
    ///
    /// All of them are shown to begin with, which is GECK's own behaviour: its
    /// Object Window lists every plugin's objects. The flags are what the
    /// window's source dropdown writes, and they are what the list is built
    /// from — including the plugin being edited, which can be hidden like any
    /// other, because a modder narrowing the list to a master has said they
    /// want the master's objects, not their own plugin's rows.
    ///
    /// It lives here rather than in the panel's filters because it is the
    /// session's own load order, indexed by it: a panel that kept its own copy
    /// would have to be told every time a session was opened, and would drift
    /// the moment a session with more plugins was opened next.
    palette_sources: Vec<bool>,
    /// The flags the current `palette_groups` was built from.
    ///
    /// Separate from [`Session::palette_sources`] because that one is the live
    /// state: the dropdown writes it, and if it were also the cache key the
    /// write would compare equal to itself next frame and never rebuild the
    /// list. This is what the last rebuild actually used, so the next one
    /// knows whether the sources moved.
    palette_sources_built: Vec<bool>,
    /// Records the session has queued for deletion, in the order asked.
    deleted: Vec<u32>,
    /// How many copies of each base object are placed in the game's cells,
    /// and how many records name it in their own definitions.
    ///
    /// The object window's two usage columns. Both need a walk of the whole
    /// load order — the references table for the copies, every record's fields
    /// for the users — which is the editor's most expensive query after the
    /// load order itself. The counts are built once, lazily, on the first
    /// column that is read, and rebuilt when the session's own records change:
    /// an edit that adds a reference has to show in the count, the same way an
    /// edit to a record's Editor ID has to show in the list.
    usage: Option<UsageCounts>,
    /// Whether the usage counts have to be rebuilt.
    ///
    /// The same idea as [`Session::stale`], kept apart because the two caches
    /// have different costs: the object window's list is hundreds of
    /// milliseconds, and the usage walks are seconds, so they are not thrown
    /// away on every keystroke — only on a write that changes what they count.
    usage_stale: bool,
    /// Status line for the UI.
    pub status: String,
    /// Whether the document has changed since the panels last read it.
    ///
    /// The object window's list and the object editor's grid both cache what
    /// they read, and the cache key is the filter — so an edit to a record
    /// leaves both showing the record as it was. This flag is the cache key
    /// they were missing: set by every write to the document, cleared by
    /// every read that honours it.
    pub stale: bool,
    persistent_by_grid: HashMap<(u32, (i32, i32)), Vec<u32>>,
    /// Any object record currently open in the editor window: (FormID, Record Sig).
    pub editing_object: Option<(u32, [u8; 4])>,
    records_cache: RwLock<HashMap<[u8; 4], CachedRecordList>>,
    spells_cache: RwLock<Option<CachedRecordList>>,
    items_cache: RwLock<Option<CachedRecordList>>,
    crea_templates_cache: RwLock<Option<CachedRecordList>>,
    npc_templates_cache: RwLock<Option<CachedRecordList>>,
}

impl Session {
    /// Open a plugin for editing, with the masters it depends on already loaded.
    ///
    /// Which game this is comes from the folder the plugin lives in
    /// ([`crate::game::GameProfile::detect`]), so a caller that already knows the
    /// Data folder needs no extra argument and a session opened by hand — from a
    /// test or an example — names its own game. The game decides the
    /// object-window group table and what the panels call the game; the format
    /// work itself is shared, so a game with no schemas yet fails at
    /// `Schema::load_dir` rather than misreading its master.
    pub fn open(
        path: &std::path::Path,
        schema_dir: &std::path::Path,
        masters: Vec<String>,
    ) -> Result<Session, String> {
        // The plugin's own folder is the Data folder the launcher chose.
        let dir = path.parent().unwrap_or(std::path::Path::new("."));
        let game = crate::game::GameProfile::detect(dir);
        let schema = load_schema(schema_dir, game)?;
        // Masters are named as file names and resolved beside the plugin being
        // edited, so `--masters FalloutNV.esm` works whatever the working folder.
        let mut plugins: Vec<nvre_esm::Plugin> = Vec::new();
        for m in &masters {
            let p = dir.join(m);
            plugins.push(nvre_esm::Plugin::open(&p).map_err(|e| format!("{m}: {e}"))?);
        }
        let doc = Document::open(path).map_err(|e| e.to_string())?;
        let own = doc.name().to_string();
        // The plugin being edited goes after its masters, unless it is one of
        // them (editing a master directly), in which case it is already loaded.
        if !plugins.iter().any(|p| p.name.eq_ignore_ascii_case(&own)) {
            plugins.push(nvre_esm::Plugin::open(path).map_err(|e| format!("{own}: {e}"))?);
        }
        let leaked_plugins: &'static [nvre_esm::Plugin] = Box::leak(plugins.into_boxed_slice());
        let lo = LoadOrder::new(leaked_plugins).map_err(|e| e.to_string())?;
        let strings = crate::strings::load(game, dir, leaked_plugins);
        let index = nvre_world::WorldIndex::build_with_strings(&lo, &schema, Some(&strings));

        let mut persistent_by_grid: HashMap<(u32, (i32, i32)), Vec<u32>> = HashMap::new();
        for (&w_fid, ws) in &index.worldspaces {
            if let Some(pc_id) = ws.persistent_cell {
                if let Some(pc) = index.cells.get(&pc_id) {
                    for &r in &pc.refs {
                        if let Some(p) = nvre_world::placed::placement(&lo, &schema, r) {
                            let g = nvre_world::cell_of(p.position[0], p.position[1]);
                            persistent_by_grid.entry((w_fid, g)).or_default().push(r);
                        }
                    }
                }
            }
        }

        // The header the file already has: the save keeps these rather than
        // blanking an author the mod came with. Read before `doc` is moved into
        // the session.
        let author = doc.plugin().header.author.clone();
        let description = doc.plugin().header.description.clone();
        // Every plugin in the load order is shown to begin with, which is what
        // the object window opens with and what GECK's own window does.
        let source_count = leaked_plugins.len();

        Ok(Session {
            game: *game,
            // The plugin's own folder, which is the Data folder the launcher
            // chose. A save has to land here: a bare file name would be
            // resolved against wherever the editor was launched from, and the
            // mod would never appear in the game's load order.
            data_dir: dir.to_path_buf(),
            doc,
            plugins: leaked_plugins,
            lo,
            schema,
            strings,
            index,
            masters,
            author,
            description,
            cell: None,
            selected: None,
            placed_index: 0,
            refs: Vec::new(),
            refs_cell: None,
            exterior_refs: Vec::new(),
            exterior_refs_key: None,
            models: HashMap::new(),
            lights: HashMap::new(),
            details: RefDetails::default(),
            details_cell: None,
            palette_groups: Vec::new(),
            palette_filter: String::new(),
            palette_sources: vec![true; source_count],
            palette_sources_built: Vec::new(),
            deleted: Vec::new(),
            usage: None,
            usage_stale: false,
            status: String::new(),
            stale: false,
            persistent_by_grid,
            editing_object: None,
            records_cache: RwLock::new(HashMap::new()),
            spells_cache: RwLock::new(None),
            items_cache: RwLock::new(None),
            crea_templates_cache: RwLock::new(None),
            npc_templates_cache: RwLock::new(None),
        })
    }

    /// Create a new, blank plugin (.esp or .esm) with the chosen masters.
    ///
    /// The game is detected from `data_dir`, the same way [`Session::open`]
    /// detects it from the plugin's folder.
    pub fn create(
        name: &str,
        is_master: bool,
        data_dir: &std::path::Path,
        schema_dir: &std::path::Path,
        masters: Vec<String>,
        author: Option<&str>,
        description: Option<&str>,
    ) -> Result<Session, String> {
        let game = crate::game::GameProfile::detect(data_dir);
        let schema = load_schema(schema_dir, game)?;
        let mut plugins: Vec<nvre_esm::Plugin> = Vec::new();
        for m in &masters {
            let p = data_dir.join(m);
            plugins.push(nvre_esm::Plugin::open(&p).map_err(|e| format!("{m}: {e}"))?);
        }
        let leaked_plugins: &'static [nvre_esm::Plugin] = Box::leak(plugins.into_boxed_slice());
        let lo = LoadOrder::new(leaked_plugins).map_err(|e| e.to_string())?;
        let strings = crate::strings::load(game, data_dir, leaked_plugins);
        let index = nvre_world::WorldIndex::build_with_strings(&lo, &schema, Some(&strings));

        let mut persistent_by_grid: HashMap<(u32, (i32, i32)), Vec<u32>> = HashMap::new();
        for (&w_fid, ws) in &index.worldspaces {
            if let Some(pc_id) = ws.persistent_cell {
                if let Some(pc) = index.cells.get(&pc_id) {
                    for &r in &pc.refs {
                        if let Some(p) = nvre_world::placed::placement(&lo, &schema, r) {
                            let g = nvre_world::cell_of(p.position[0], p.position[1]);
                            persistent_by_grid.entry((w_fid, g)).or_default().push(r);
                        }
                    }
                }
            }
        }

        let doc = Document::create_empty(name, is_master, &masters, author, description)
            .map_err(|e| e.to_string())?;
        // Every plugin in the load order is shown to begin with, which is what
        // the object window opens with and what GECK's own window does.
        let source_count = leaked_plugins.len();

        Ok(Session {
            game: *game,
            // The folder the new plugin is created in, which is also where it
            // is saved: the game's Data folder, so the game finds it.
            data_dir: data_dir.to_path_buf(),
            doc,
            plugins: leaked_plugins,
            lo,
            schema,
            strings,
            index,
            masters,
            // What the new header was given, so the first save writes it back
            // and the Properties page shows it.
            author: author.map(str::to_string),
            description: description.map(str::to_string),
            cell: None,
            selected: None,
            placed_index: 0,
            refs: Vec::new(),
            refs_cell: None,
            exterior_refs: Vec::new(),
            exterior_refs_key: None,
            models: HashMap::new(),
            lights: HashMap::new(),
            details: RefDetails::default(),
            details_cell: None,
            palette_groups: Vec::new(),
            palette_filter: String::new(),
            palette_sources: vec![true; source_count],
            palette_sources_built: Vec::new(),
            deleted: Vec::new(),
            usage: None,
            usage_stale: false,
            status: format!("created new plugin {name}"),
            stale: false,
            persistent_by_grid,
            editing_object: None,
            records_cache: RwLock::new(HashMap::new()),
            spells_cache: RwLock::new(None),
            items_cache: RwLock::new(None),
            crea_templates_cache: RwLock::new(None),
            npc_templates_cache: RwLock::new(None),
        })
    }

    /// The plugin being edited.
    pub fn plugin(&self) -> &nvre_esm::Plugin {
        self.doc.plugin()
    }

    /// The load order, pre-built at session startup (zero cost lookup).
    pub fn load_order(&self) -> Result<&LoadOrder<'static>, String> {
        Ok(&self.lo)
    }

    /// The model a base object's reference is drawn with, cached.
    ///
    /// The answer is cached per base object, but a *miss* still builds a
    /// `LoadOrder` — which walks every record in every plugin, hundreds of
    /// milliseconds. Callers drawing a whole cell should use
    /// [`Session::model_paths_for`], which builds it once and shares it across
    /// every miss in the batch.
    pub fn model_path_for(&mut self, base: u32) -> Option<String> {
        if let Some(hit) = self.models.get(&base) {
            return hit.clone();
        }
        let found = self.resolve_model_path(base);
        self.models.insert(base, found.clone());
        found
    }

    /// Resolve many base objects against one `LoadOrder`.
    ///
    /// Building a `LoadOrder` is the expensive part of resolving a model path, so
    /// a cell's worth of references is resolved in one pass rather than one pass
    /// each. Uncached entries are filled in as they are found.
    pub fn model_paths_for(&mut self, bases: &[u32]) -> Vec<Option<String>> {
        let missing: Vec<u32> = bases
            .iter()
            .copied()
            .filter(|b| !self.models.contains_key(b))
            .collect();
        if !missing.is_empty() {
            // Computed outside the borrow of the cache, which needs `&mut self`.
            let resolved = (|| {
                let lo = self.load_order().ok()?;
                Some(
                    missing
                        .iter()
                        .map(|&b| {
                            nvre_world::placed::base_object(lo, &self.schema, b)
                                .and_then(|o| o.world_model_path().or_else(|| o.model_path()))
                        })
                        .collect::<Vec<_>>(),
                )
            })();
            if let Some(resolved) = resolved {
                for (b, path) in missing.iter().zip(resolved) {
                    self.models.insert(*b, path);
                }
            } else {
                // A load order that will not resolve: record the failure so it is
                // not retried every frame.
                for b in &missing {
                    self.models.insert(*b, None);
                }
            }
        }
        bases
            .iter()
            .map(|b| self.models.get(b).cloned().flatten())
            .collect()
    }

    /// A base object's model path, with no caching.
    fn resolve_model_path(&mut self, base: u32) -> Option<String> {
        let lo = self.load_order().ok()?;
        let b = nvre_world::placed::base_object(lo, &self.schema, base)?;
        b.world_model_path().or_else(|| b.model_path())
    }

    /// The `LIGH` data a base object's reference is lit by, cached.
    ///
    /// `None` means the base object is not a light at all. A light whose `DATA`
    /// this game's schema does not decode still answers — with
    /// [`LightData::UNKNOWN`] — because the reference *is* a light, and a cell
    /// full of lamps should not render as if it had none.
    ///
    /// Resolving one base object builds a `LoadOrder`, which is hundreds of
    /// milliseconds, so a cell's worth of references belongs in
    /// [`Session::light_paths_for`].
    pub fn light_for(&mut self, base: u32) -> Option<LightData> {
        if let Some(hit) = self.lights.get(&base) {
            return hit.clone();
        }
        let found = self.resolve_light(base);
        self.lights.insert(base, found.clone());
        found
    }

    /// A base object's light data, with no caching.
    fn resolve_light(&mut self, base: u32) -> Option<LightData> {
        let lo = self.load_order().ok()?;
        let b = nvre_world::placed::base_object(lo, &self.schema, base)?;
        if b.sig != *b"LIGH" {
            return None;
        }
        // The `DATA` subrecord is the whole of it: colour, radius, flags.
        let (_, rec) = nvre_world::fields::winner(lo, base)?;
        light_data_in(rec, &self.schema)
    }

    /// Resolve many base objects' light data against one `LoadOrder`.
    ///
    /// The same shape as [`Session::model_paths_for`]: the walk is the
    /// expensive part, so a cell's references are resolved in one pass.
    pub fn light_paths_for(&mut self, bases: &[u32]) -> Vec<Option<LightData>> {
        let mut out = Vec::with_capacity(bases.len());
        // Everything cached is answered without a walk; only the misses cost.
        let misses: Vec<u32> = bases
            .iter()
            .copied()
            .filter(|b| !self.lights.contains_key(b))
            .collect();
        if !misses.is_empty() {
            // Resolved outside the borrow of the cache, which needs `&mut self`.
            let resolved = (|| {
                let lo = self.load_order().ok()?;
                Some(
                    misses
                        .iter()
                        .map(|&b| {
                            let is_ligh = nvre_world::placed::base_object(lo, &self.schema, b)
                                .is_some_and(|o| o.sig == *b"LIGH");
                            if !is_ligh {
                                return None;
                            }
                            let (_, rec) = nvre_world::fields::winner(lo, b)?;
                            light_data_in(rec, &self.schema)
                        })
                        .collect::<Vec<_>>(),
                )
            })();
            if let Some(resolved) = resolved {
                for (b, light) in misses.iter().zip(resolved) {
                    self.lights.insert(*b, light);
                }
            } else {
                // A load order that will not resolve: record the failure so it is
                // not retried every frame.
                for b in &misses {
                    self.lights.insert(*b, None);
                }
            }
        }
        for base in bases {
            out.push(self.lights.get(base).cloned().flatten());
        }
        out
    }

    /// How many of these base objects are lights.
    ///
    /// The viewport asks before spawning anything, so a cell with no lights does
    /// not walk the load order to discover that.
    pub fn lights_in(&mut self, bases: &[u32]) -> usize {
        self.light_paths_for(bases).iter().flatten().count()
    }

    /// One subrecord of a record's winning definition, as its raw bytes.
    ///
    /// The record walk already resolves the winner; this is the accessor a
    /// diagnostic wants when it needs a subrecord the readers do not carry.
    pub fn record_sub(&mut self, form_id: u32, sig: &[u8; 4]) -> Option<Vec<u8>> {
        let lo = self.load_order().ok()?;
        let (_, rec) = nvre_world::fields::winner(lo, form_id)?;
        rec.subrecords()
            .ok()?
            .into_iter()
            .find(|s| &s.sig == sig)
            .map(|s| s.data)
    }

    /// The open cell's own lighting, if it has any.
    ///
    /// The cell's `XCLL` is the ambient colour, the sun's colour and the fog
    /// the renderer draws with. If a cell has a Light Template (`LTMP`) and
    /// inherit flags (`LNAM`), inherited fields are populated from the template's
    /// `LGTM` record (`DATA` and optional `DALC`).
    ///
    /// An exterior cell carries no `XCLL` of its own — it resolves its lighting
    /// from the active Weather (`WTHR`) via Climate (`CLMT`) / Worldspace (`WRLD`),
    /// sampling daylight ambient, directional sunlight, and fog.
    pub fn cell_lighting(&mut self) -> Option<CellLighting> {
        let cell = self.cell?;
        let xcll_bytes = self.cell_sub(cell, b"XCLL");
        let ltmp_fid = self.cell_sub(cell, b"LTMP").and_then(|b| {
            if b.len() >= 4 {
                Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            } else {
                None
            }
        });
        let lnam_flags = self
            .cell_sub(cell, b"LNAM")
            .and_then(|b| {
                if b.len() >= 4 {
                    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                } else {
                    None
                }
            })
            .unwrap_or(0);

        // If we have an LTMP template, read its LGTM DATA & DALC
        let template_lighting = if let Some(fid) = ltmp_fid {
            let lgtm_data = self.record_sub(fid, b"DATA");
            let mut l = lgtm_data.as_deref().and_then(cell_lighting);
            if let Some(ref mut tl) = l {
                // If Skyrim/FO4 LGTM has a separate DALC subrecord:
                if tl.directional_ambient.is_none() {
                    if let Some(dalc_bytes) = self.record_sub(fid, b"DALC") {
                        if dalc_bytes.len() >= 24 {
                            let mut dalc = [[0u8; 3]; 6];
                            for i in 0..6 {
                                dalc[i] = [
                                    dalc_bytes[i * 3],
                                    dalc_bytes[i * 3 + 1],
                                    dalc_bytes[i * 3 + 2],
                                ];
                            }
                            tl.directional_ambient = Some(dalc);
                        }
                    }
                }
            }
            l
        } else {
            None
        };

        match (
            xcll_bytes.as_deref().and_then(cell_lighting),
            template_lighting,
        ) {
            (Some(mut cell_l), Some(tmpl_l)) => {
                // Apply LNAM inheritance flags:
                // Bit 0: Ambient Color
                if (lnam_flags & (1 << 0)) != 0 {
                    cell_l.ambient = tmpl_l.ambient;
                    if tmpl_l.directional_ambient.is_some() {
                        cell_l.directional_ambient = tmpl_l.directional_ambient;
                        cell_l.directional_scale = tmpl_l.directional_scale;
                        cell_l.specular = tmpl_l.specular;
                    }
                }
                // Bit 1: Directional Color
                if (lnam_flags & (1 << 1)) != 0 {
                    cell_l.directional = tmpl_l.directional;
                }
                // Bit 2: Fog Color
                if (lnam_flags & (1 << 2)) != 0 {
                    cell_l.fog = tmpl_l.fog;
                }
                // Bit 3: Fog Near
                if (lnam_flags & (1 << 3)) != 0 {
                    cell_l.fog_near = tmpl_l.fog_near;
                }
                // Bit 4: Fog Far
                if (lnam_flags & (1 << 4)) != 0 {
                    cell_l.fog_far = tmpl_l.fog_far;
                }
                // Bit 5: Directional Rotation
                if (lnam_flags & (1 << 5)) != 0 {
                    cell_l.rotation_xy = tmpl_l.rotation_xy;
                    cell_l.rotation_z = tmpl_l.rotation_z;
                }
                // Bit 6: Directional Fade
                if (lnam_flags & (1 << 6)) != 0 {
                    cell_l.directional_fade = tmpl_l.directional_fade;
                }
                // Bit 7: Clip Distance
                if (lnam_flags & (1 << 7)) != 0 {
                    cell_l.fog_clip_dist = tmpl_l.fog_clip_dist;
                }
                // Bit 8: Fog Power
                if (lnam_flags & (1 << 8)) != 0 {
                    cell_l.fog_power = tmpl_l.fog_power;
                }
                Some(cell_l)
            }
            (Some(cell_l), None) => Some(cell_l),
            (None, Some(tmpl_l)) => Some(tmpl_l),
            (None, None) => {
                // If this is an exterior cell, attempt to resolve daylight lighting from Weather (WTHR)
                self.exterior_weather_lighting(cell)
            }
        }
    }

    /// Resolve lighting for an exterior cell from the active Weather (WTHR) record.
    fn exterior_weather_lighting(&mut self, cell: u32) -> Option<CellLighting> {
        let is_interior = self.index.cells.get(&cell).is_some_and(|c| c.is_interior());
        if is_interior {
            return None;
        }

        // Find a representative WTHR record in the load order
        let weathers = self.records_by_sig(*b"WTHR");
        let wthr_fid = weathers.first()?.0;
        let nam0 = self.record_sub(wthr_fid, b"NAM0")?;

        // NAM0 holds 10 color types. Each color type has times_of_day colors (4 bytes per color: RGB + pad).
        // times_of_day is typically 6 (Sunrise, Day, Sunset, Night, High Noon, Midnight) or 4.
        let times = if nam0.len() % 240 == 0 {
            6
        } else if nam0.len() % 160 == 0 {
            4
        } else {
            return None;
        };
        let type_stride = times * 4;
        if nam0.len() < type_stride * 10 {
            return None;
        }

        // Index 1 = Day (time of day index 1)
        let day_idx = 1.min(times - 1);
        let color_at = |type_idx: usize| -> [u8; 3] {
            let offset = type_idx * type_stride + day_idx * 4;
            [nam0[offset], nam0[offset + 1], nam0[offset + 2]]
        };

        // Color types: 1 = Fog, 3 = Ambient, 4 = Sunlight
        let fog = color_at(1);
        let ambient = color_at(3);
        let sunlight = color_at(4);

        // Optional FNAM fog distances (6x f32: day near, day far, night near, night far, etc.)
        let (fog_near, fog_far) = if let Some(fnam) = self.record_sub(wthr_fid, b"FNAM") {
            if fnam.len() >= 8 {
                let near = f32::from_le_bytes([fnam[0], fnam[1], fnam[2], fnam[3]]);
                let far = f32::from_le_bytes([fnam[4], fnam[5], fnam[6], fnam[7]]);
                (near, far)
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        };

        Some(CellLighting {
            ambient,
            directional: sunlight,
            fog,
            fog_near,
            fog_far,
            rotation_xy: 45, // default pleasant daylight sun angle
            rotation_z: 60,
            directional_fade: 1.0,
            fog_clip_dist: 0.0,
            fog_power: 1.0,
            directional_ambient: None,
            specular: None,
            directional_scale: None,
        })
    }

    /// The object palette, grouped into the schema's categories.
    ///
    /// GECK's Object Window is a category list on the left and that category's
    /// objects on the right, so the palette is built the same way: one group per
    /// record type, named the way the game's editor names it ("Static",
    /// "Container"), each holding its objects sorted by name.
    ///
    /// What is listed is the schema's answer with no fallback: a signature the
    /// object-window table does not name, or names as hidden, is not shown —
    /// placed references, cells and the plugin header are all records, and the
    /// table says which records are objects.
    ///
    /// The query walks the load order, so it is worth hundreds of milliseconds;
    /// the panel asks for it every frame otherwise, and the result is cached
    /// until the filter changes.
    ///
    /// The walk is the load order's, but what it lists is the *mod's*: a
    /// record this session has edited is shown with the edit applied, and a
    /// duplicate this session has created is shown at all. Both are overlaid
    /// after the walk, and only for the FormIDs the document actually touches
    /// — a handful, against the 44,550 the walk returns — so the cost is a
    /// hash lookup per row rather than a subrecord parse per row.
    ///
    /// `own_only` narrows the list to what the plugin being edited carries:
    /// its own records on disk, plus every record this session has touched,
    /// because an edit to a master's record is a record the mod now defines
    /// even though no plugin on disk holds it yet.
    ///
    /// `sources` narrows it further, one flag per plugin in the load order:
    /// a plugin whose flag is off has its records skipped, which is how a
    /// modder hides the masters they do not care about while still seeing the
    /// game's base objects. All flags on is the whole load order, which is
    /// what the window opens with — and a plugin with no records left in it
    /// simply has no rows, the same as any other empty category.
    pub fn object_groups(&mut self, filter: &str, sources: &[bool]) -> &[ObjectGroup] {
        // `stale` is the cache key the filter alone was missing: an edit
        // changes what a row says without changing what it matches, so the
        // list has to be rebuilt even when the filter has not moved. The
        // sources are the same kind of key: they change which rows are listed
        // without changing what any of them say.
        //
        // The comparison is against `palette_sources_built`, not against
        // `palette_sources`: that one is the live state the dropdown writes,
        // and comparing the caller's flags against it would compare a value
        // against itself the frame after a write — equal, so the list would
        // never be rebuilt and unchecking a plugin would change nothing.
        if self.palette_filter != filter
            || self.palette_groups.is_empty()
            || self.stale
            || self.palette_sources_built != sources
        {
            let found = (|| {
                let lo = self.load_order().ok()?;
                // Only signatures the table names and does not hide are
                // objects; everything else is skipped before its names are
                // read, which keeps the walk off the placed references and
                // cells that make up most of a master.
                let visible: HashSet<[u8; 4]> = self
                    .schema
                    .object_window
                    .categories
                    .iter()
                    .filter(|(_, c)| !c.hidden)
                    .filter_map(|(sig, _)| sig.as_bytes().try_into().ok())
                    .collect();
                // Which plugins the list draws from, resolved once: a flag per
                // plugin, defaulting to shown for a plugin the caller did not
                // name, so a shorter list than the load order is a narrower
                // choice rather than an empty window.
                //
                // Every flag is honoured as given, the plugin being edited
                // included: a modder who narrows the list to a master has said
                // they want the master's objects, and their own plugin's rows
                // are the one thing they can already see everywhere else.
                let shown: Vec<bool> = (0..lo.plugins.len())
                    .map(|pi| sources.get(pi).copied().unwrap_or(true))
                    .collect();
                // Whether the plugin being edited is among them. Its own
                // records — a staged duplicate, an edit queued on a master's
                // record — are the mod's, so they follow its flag rather than
                // the master's: hiding the mod hides them, and showing only a
                // master shows none of them.
                let own_shown = lo
                    .plugins
                    .iter()
                    .position(|p| p.name.eq_ignore_ascii_case(self.doc.name()))
                    .and_then(|pi| shown.get(pi).copied())
                    .unwrap_or(true);
                let mut objects = load_order_objects(lo, &self.strings, filter, &visible, &shown);
                // The FormIDs the document has something to say about: an edit
                // queued on a record, or a duplicate staged this session. Only
                // these rows are re-read, which is what keeps the overlay off
                // the load-order walk's cost.
                let touched: HashSet<u32> = self
                    .doc
                    .edits()
                    .map(|(fid, _)| *fid)
                    .chain(self.doc.deleted().iter().copied())
                    .chain(
                        self.doc
                            .inserted_top()
                            .iter()
                            .map(|p| p.record.header.form_id),
                    )
                    .collect();
                for (_fid, _sig, row) in objects.iter_mut() {
                    if !touched.contains(&row.fid) {
                        continue;
                    }
                    // A record queued for deletion is not in the list at all:
                    // the game will not have it, so the window should not
                    // either.
                    if self.doc.deleted().contains(&row.fid) {
                        row.edid = String::new();
                        row.name = String::new();
                        continue;
                    }
                    if let Some(resolved) = self.resolved_object_row(row.fid) {
                        *row = resolved;
                    }
                }
                objects.retain(|(fid, _, _)| !self.doc.deleted().contains(fid));
                // A staged duplicate is a row no plugin on disk holds, so the
                // walk never saw it. It is appended here, under its own
                // signature's category, which is the same category the record
                // it was copied from is filed under — but only while the plugin
                // being edited is shown, because a duplicate is that plugin's
                // record and nothing else's.
                let staged: Vec<(u32, [u8; 4], ObjectRow)> = if own_shown {
                    self.doc
                        .inserted_top()
                        .iter()
                        .filter(|p| {
                            p.record.header.sig != *b"TES4"
                                && visible.contains(&p.record.header.sig)
                        })
                        .filter_map(|p| {
                            let row = self.resolved_object_row(p.record.header.form_id)?;
                            Some((row.fid, p.record.header.sig, row))
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                // The filter applies to the staged rows too: a duplicate is
                // listed like any other object, not shown regardless of what
                // was typed.
                let filter_lower = filter.to_lowercase();
                let staged: Vec<(u32, [u8; 4], ObjectRow)> = if filter_lower.is_empty() {
                    staged
                } else {
                    staged
                        .into_iter()
                        .filter(|(fid, _, row)| matches_text(&filter_lower, *fid, row))
                        .collect()
                };
                objects.extend(staged);
                // An edited master record is a row the walk never produced: the
                // record lives in the master, and what makes it the mod's is
                // the edit queued on it, which the walk does not read. It is
                // added here whenever the plugin being edited is shown, under
                // the category the record's own signature names. Without it, a
                // modder who edited a master's weapon and then narrowed the
                // list to their own plugin would not find the one thing they
                // changed — and with the plugin hidden, it is hidden with it,
                // because it is the mod's record and not the master's.
                if own_shown {
                    let existing: HashSet<u32> = objects.iter().map(|(fid, _, _)| *fid).collect();
                    let edited: Vec<u32> = self
                        .doc
                        .edits()
                        .filter(|(fid, _)| !existing.contains(*fid))
                        .map(|(fid, _)| *fid)
                        .collect();
                    for fid in edited {
                        let Some(row) = self.resolved_object_row(fid) else {
                            continue;
                        };
                        let sig = self
                            .resolved_record(fid)
                            .map(|r| r.header.sig)
                            .unwrap_or(*b"????");
                        if !visible.contains(&sig)
                            || !matches_text(filter_lower.as_str(), fid, &row)
                        {
                            continue;
                        }
                        objects.push((fid, sig, row));
                    }
                }
                // Grouped by the record type, which is the category. One
                // load-order lookup per distinct type rather than per object.
                //
                // Where the category is filed and what it is called come from
                // the schema's object-window table, which is per game: the
                // same signature is a different category in a different game.
                // `Schema::check` has already said a shown category names a
                // group of this game's own; the label falls back through the
                // schema's two answers — the table's name for the category,
                // then the record definition's — before the signature itself.
                let mut types: HashMap<[u8; 4], (String, String)> = HashMap::new();
                for (_, sig, _) in &objects {
                    types.entry(*sig).or_insert_with(|| {
                        let key = String::from_utf8_lossy(sig);
                        let cat = self.schema.object_window.categories.get(key.as_ref());
                        let group = cat.and_then(|c| c.group.clone()).unwrap_or_default();
                        let category = cat
                            .and_then(|c| c.name.clone())
                            .or_else(|| {
                                self.schema
                                    .records
                                    .get(key.as_ref())
                                    .map(|d| d.name.clone())
                            })
                            .unwrap_or_else(|| key.to_string());
                        (group, category)
                    });
                }
                let mut groups: Vec<ObjectGroup> = Vec::new();
                for (_fid, sig, row) in objects {
                    let Some((group, category)) = types.get(&sig).cloned() else {
                        continue;
                    };
                    let at = match groups.iter().position(|g| g.category == category) {
                        Some(i) => i,
                        None => {
                            groups.push(ObjectGroup {
                                group,
                                category,
                                sig,
                                objects: Vec::new(),
                            });
                            groups.len() - 1
                        }
                    };
                    groups[at].objects.push(row);
                }
                // Categories ordered by GECK top-level group, then alphabetically by category.
                let order = &self.schema.object_window.groups;
                groups.sort_by(|a, b| {
                    group_rank(order, &a.group)
                        .cmp(&group_rank(order, &b.group))
                        .then_with(|| a.category.cmp(&b.category))
                });
                for g in &mut groups {
                    g.objects.sort_by(|a, b| {
                        a.label()
                            .cmp(b.label())
                            .then_with(|| a.edid.cmp(&b.edid))
                            .then(a.fid.cmp(&b.fid))
                    });
                }
                Some(groups)
            })()
            .unwrap_or_default();
            self.palette_groups = found;
            self.palette_filter = filter.to_string();
            self.palette_sources_built = sources.to_vec();
            self.stale = false;
        }
        &self.palette_groups
    }

    /// The load order's plugin names, in load order, for the object window's
    /// source list.
    ///
    /// Indexed by the same numbers the list's checkboxes are, so the panel
    /// draws one row per plugin without knowing anything about the load order
    /// itself.
    pub fn object_sources(&self) -> Vec<(usize, &str)> {
        self.lo
            .plugins
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.name.as_str()))
            .collect()
    }

    /// Which plugins the object window's list draws from, one flag per plugin
    /// in load order.
    ///
    /// Borrowed rather than cloned, because the panel reads it every frame to
    /// draw the list and only writes it when a checkbox is clicked.
    pub fn object_source_flags(&self) -> &[bool] {
        &self.palette_sources
    }

    /// Turn one plugin's contribution to the object window on or off.
    ///
    /// Any plugin, the plugin being edited included: a modder narrowing the
    /// list to a master has said they want the master's objects, and their own
    /// plugin's rows are the one thing they can already see everywhere else.
    /// Hiding it leaves the window showing the masters' objects and nothing
    /// else, which is a narrower list rather than a broken one — the empty
    /// state says so, rather than a category that silently refills.
    pub fn set_object_source(&mut self, index: usize, shown: bool) -> Result<(), String> {
        if index >= self.palette_sources.len() {
            return Err(format!("no plugin {index} in the load order"));
        }
        self.palette_sources[index] = shown;
        Ok(())
    }

    /// Show every plugin's objects, or only the plugin being edited's.
    ///
    /// The two ends of the source list, as one call: "all plugins" is GECK's
    /// own default, and "this plugin" is the narrowest list that still shows
    /// what the modder is working on.
    pub fn set_object_sources_all(&mut self, all: bool) {
        let own = self.doc.name().to_string();
        for (pi, p) in self.lo.plugins.iter().enumerate() {
            self.palette_sources[pi] = all || p.name.eq_ignore_ascii_case(&own);
        }
    }

    /// Return all records of a given signature in the load order, sorted by Editor ID (cached).
    pub fn records_by_sig(&self, sig: [u8; 4]) -> Arc<[(u32, String)]> {
        if let Ok(cache) = self.records_cache.read() {
            if let Some(hits) = cache.get(&sig) {
                return hits.clone();
            }
        }

        let mut out: Vec<(u32, String)> = Vec::new();
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for (pi, plugin) in self.lo.plugins.iter().enumerate() {
            let strings = self
                .strings
                .for_plugin(&plugin.name, plugin.header.is_localized());
            for r in &self.lo.records[pi] {
                if r.header.sig == sig {
                    if let Some(global) = self.lo.to_global(pi, r.header.form_id) {
                        if seen.insert(global) {
                            let (edid, full) =
                                nvre_esm::inventory::names(r, strings).unwrap_or((None, None));
                            let name = edid.or(full).unwrap_or_else(|| format!("{:08X}", global));
                            out.push((global, name));
                        }
                    }
                }
            }
        }
        out.sort_by_key(|a| a.1.to_lowercase());
        let arc: Arc<[(u32, String)]> = out.into();
        if let Ok(mut cache) = self.records_cache.write() {
            cache.insert(sig, arc.clone());
        }
        arc
    }

    /// Return all FormList records (FLST) across the load order, sorted by Editor ID (cached).
    pub fn form_lists(&self) -> Arc<[(u32, String)]> {
        self.records_by_sig(*b"FLST")
    }

    /// Return all Spells and Enchantments across the load order, sorted by Editor ID (cached).
    pub fn spells_and_enchantments(&self) -> Arc<[(u32, String)]> {
        if let Ok(cache) = self.spells_cache.read() {
            if let Some(hits) = cache.as_ref() {
                return hits.clone();
            }
        }

        let mut spells_all = self.records_by_sig(*b"SPEL").to_vec();
        let mut enchs = self.records_by_sig(*b"ENCH").to_vec();
        spells_all.append(&mut enchs);
        spells_all.sort_by_key(|a| a.1.to_lowercase());
        let arc: Arc<[(u32, String)]> = spells_all.into();
        if let Ok(mut cache) = self.spells_cache.write() {
            *cache = Some(arc.clone());
        }
        arc
    }

    /// Return all inventory items across the load order, sorted by Editor ID (cached).
    pub fn inventory_items(&self) -> Arc<[(u32, String)]> {
        if let Ok(cache) = self.items_cache.read() {
            if let Some(hits) = cache.as_ref() {
                return hits.clone();
            }
        }

        let mut items_all = Vec::new();
        for sig in [
            b"WEAP", b"ARMO", b"ALCH", b"AMMO", b"BOOK", b"MISC", b"LVLI",
        ] {
            let sub = self.records_by_sig(*sig);
            items_all.extend_from_slice(&sub);
        }
        items_all.sort_by_key(|a| a.1.to_lowercase());
        let arc: Arc<[(u32, String)]> = items_all.into();
        if let Ok(mut cache) = self.items_cache.write() {
            *cache = Some(arc.clone());
        }
        arc
    }

    /// Return all creature templates (CREA + LVLC) across the load order, sorted by Editor ID (cached).
    pub fn creature_templates(&self) -> Arc<[(u32, String)]> {
        if let Ok(cache) = self.crea_templates_cache.read() {
            if let Some(hits) = cache.as_ref() {
                return hits.clone();
            }
        }

        let mut c = self.records_by_sig(*b"CREA").to_vec();
        let mut lvlc = self.records_by_sig(*b"LVLC").to_vec();
        c.append(&mut lvlc);
        c.sort_by_key(|a| a.1.to_lowercase());
        let arc: Arc<[(u32, String)]> = c.into();
        if let Ok(mut cache) = self.crea_templates_cache.write() {
            *cache = Some(arc.clone());
        }
        arc
    }

    /// Return all NPC templates (NPC_ + LVLN) across the load order, sorted by Editor ID (cached).
    pub fn npc_templates(&self) -> Arc<[(u32, String)]> {
        if let Ok(cache) = self.npc_templates_cache.read() {
            if let Some(hits) = cache.as_ref() {
                return hits.clone();
            }
        }

        let mut n = self.records_by_sig(*b"NPC_").to_vec();
        let mut lvln = self.records_by_sig(*b"LVLN").to_vec();
        n.append(&mut lvln);
        n.sort_by_key(|a| a.1.to_lowercase());
        let arc: Arc<[(u32, String)]> = n.into();
        if let Ok(mut cache) = self.npc_templates_cache.write() {
            *cache = Some(arc.clone());
        }
        arc
    }

    /// The worldspaces in the open plugin and its masters, as browser rows.
    ///
    /// This is the cell browser's dropdown: GECK's Cell View lists every
    /// worldspace and an "Interior" entry, and the list below it is filtered by
    /// whichever one is chosen.
    pub fn worldspaces(&self) -> Vec<WorldspaceRow> {
        let mut out: Vec<WorldspaceRow> = self
            .index
            .worldspaces
            .values()
            .map(|w| WorldspaceRow {
                form_id: w.form_id,
                label: w
                    .edid
                    .clone()
                    .or_else(|| w.name.clone())
                    .unwrap_or_else(|| format!("{:08X}", w.form_id)),
                name: w.name.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.label.cmp(&b.label).then(a.form_id.cmp(&b.form_id)));
        out
    }

    /// The cells the browser lists under one scope, as browser rows.
    ///
    /// A scope is either the interiors or one worldspace's exterior cells. The
    /// `worldspace` filter matches the world a CELL record sits under, which is
    /// what the game loads: a worldspace's own persistent cell is in it too, so
    /// it appears in that worldspace's list rather than in the interiors'.
    pub fn cells(&self, scope: CellScope) -> Vec<CellRow> {
        let mut out: Vec<CellRow> = self
            .index
            .cells
            .values()
            .filter(|c| match scope {
                CellScope::Interiors => c.world.is_none(),
                CellScope::Worldspace(w) => c.world == Some(w),
            })
            .map(|c| CellRow {
                form_id: c.form_id,
                edid: c.edid.clone(),
                name: c.name.clone(),
                interior: c.is_interior(),
                refs: c.refs.len(),
                world: c.world.is_some(),
                grid: c.grid,
            })
            .collect();
        out.sort_by(|a, b| a.edid.cmp(&b.edid).then(a.form_id.cmp(&b.form_id)));
        out
    }

    /// The references in the open cell, in file order.
    ///
    /// Built once per cell and cached: this is read by several systems every
    /// frame, and rebuilding it means decoding every reference's NAME subrecord
    /// and allocating a string per field, per frame.
    pub fn cell_refs(&mut self) -> &[RefRow] {
        let Some(cell) = self.cell else {
            return &[];
        };
        if self.refs_cell != Some(cell) {
            self.refs = self.build_cell_refs(cell);
            self.refs_cell = Some(cell);
        }
        &self.refs
    }

    /// The uncached build, for measuring what the cache costs.
    pub fn build_cell_refs_pub(&self, cell: u32) -> Vec<RefRow> {
        self.build_cell_refs(cell)
    }

    /// One subrecord of a cell's winning record, as its raw bytes.
    ///
    /// The cell's own data lives in its `CELL` record, which the index points
    /// at but does not keep. This walks the load order for the winner and
    /// hands back the subrecord's payload, which is what a reader that wants
    /// a field the index does not carry needs.
    ///
    /// `None` when the cell has no such subrecord, or its record is missing.
    pub fn cell_sub(&mut self, cell: u32, sig: &[u8; 4]) -> Option<Vec<u8>> {
        let lo = self.load_order().ok()?;
        let (_, rec) = nvre_world::fields::winner(lo, cell)?;
        rec.subrecords()
            .ok()?
            .into_iter()
            .find(|s| &s.sig == sig)
            .map(|s| s.data)
    }

    /// Collect references for the active cell and its exterior grid neighbors within `radius`.
    pub fn exterior_grid_refs(&mut self, radius: i32) -> &[RefRow] {
        let Some(cell) = self.cell else {
            return &[];
        };
        if self.exterior_refs_key == Some((cell, radius)) {
            return &self.exterior_refs;
        }

        let Some(c) = self.index.cells.get(&cell) else {
            return &[];
        };
        let (Some(w), Some(g)) = (c.world, c.grid) else {
            return self.cell_refs();
        };
        if radius == 0 {
            return self.cell_refs();
        }
        let Some(ws) = self.index.worldspaces.get(&w).cloned() else {
            return self.cell_refs();
        };

        let mut all_rows = Vec::new();
        if radius < 0 {
            // "All" / Whole Worldspace: load references for all cells in the worldspace.
            for &cid in ws.grid.values() {
                if cid == cell {
                    all_rows.extend(self.cell_refs().iter().cloned());
                } else {
                    all_rows.extend(self.build_cell_refs(cid));
                }
            }
        } else {
            for dx in -radius..=radius {
                for dy in -radius..=radius {
                    let coord = (g.0 + dx, g.1 + dy);
                    if let Some(&cid) = ws.grid.get(&coord) {
                        if cid == cell {
                            all_rows.extend(self.cell_refs().iter().cloned());
                        } else {
                            all_rows.extend(self.build_cell_refs(cid));
                        }
                    }
                }
            }
        }
        self.exterior_refs = all_rows;
        self.exterior_refs_key = Some((cell, radius));
        &self.exterior_refs
    }

    /// Calculate the expected number of cells in an exterior grid of radius `r`: `(2r + 1)^2`.
    pub fn grid_cell_count(radius: i32) -> usize {
        if radius < 0 {
            0
        } else {
            let side = (2 * radius + 1) as usize;
            side * side
        }
    }

    /// The references table's resolved columns, for the open cell.
    ///
    /// An owner and a base object's type both need the load order, which costs
    /// hundreds of milliseconds to build. The panel draws every frame, so the
    /// answers are cached per cell rather than resolved per row per frame: a
    /// cell names a handful of owners, and one load order serves them all.
    pub fn ref_details(&mut self) -> &RefDetails {
        let Some(cell) = self.cell else {
            return &EMPTY_DETAILS;
        };
        if self.details_cell != Some(cell) {
            self.details = self.build_ref_details(cell);
            self.details_cell = Some(cell);
        }
        &self.details
    }

    fn build_ref_details(&mut self, cell: u32) -> RefDetails {
        // The rows are read out first: the resolution needs `&mut self`, and
        // the rows borrow `self.refs`.
        let rows: Vec<RefRow> = self.build_cell_refs(cell);
        let Some(lo) = self.load_order().ok() else {
            return RefDetails::default();
        };
        let mut owners: HashMap<u32, String> = HashMap::new();
        let mut types: HashMap<u32, String> = HashMap::new();
        let mut bases: HashMap<u32, String> = HashMap::new();
        for r in &rows {
            if let Some(owner) = r.owner {
                owners.entry(owner).or_insert_with(|| {
                    nvre_world::fields::winner(lo, owner)
                        .and_then(|(_, rec)| nvre_edit::document::edid_of(rec))
                        .unwrap_or_else(|| format!("{owner:08X}"))
                });
            }
            // The type column says what the reference places, not what it is:
            // a REFR is always a placed object, so the base's record type is
            // the interesting half.
            //
            // It is the schema's own name for that record — "Static", "Misc.
            // Item" — which is what GECK shows, and which doubles as the type
            // dropdown's identity, since no two record types share a name.
            if let Some(base) = r
                .base
                .as_deref()
                .and_then(|b| u32::from_str_radix(b.strip_prefix("0x")?, 16).ok())
            {
                if let Some((_, rec)) = nvre_world::fields::winner(lo, base) {
                    types.entry(base).or_insert_with(|| {
                        self.schema
                            .records
                            .get(rec.sig_str().as_str())
                            .map(|d| d.name.clone())
                            .unwrap_or_else(|| rec.sig_str())
                    });
                    // And the base's Editor ID, which is what a placed object is
                    // called: a reference's own EDID is set on 5,011 of
                    // FalloutNV's 307,710 REFRs [observed: fnvtool esm survey], so
                    // naming a row by its own would leave 98% of them unnamed.
                    bases.entry(base).or_insert_with(|| {
                        nvre_edit::document::edid_of(rec).unwrap_or_else(|| format!("{base:08X}"))
                    });
                }
            }
        }
        RefDetails {
            owners,
            types,
            bases,
        }
    }

    fn build_cell_refs(&self, cell: u32) -> Vec<RefRow> {
        let Some(c) = self.index.cells.get(&cell) else {
            return Vec::new();
        };
        let mut ref_ids: Vec<u32> = c.refs.clone();
        if let (Some(w), Some(g)) = (c.world, c.grid) {
            if let Some(p) = self.persistent_by_grid.get(&(w, g)) {
                ref_ids.extend(p);
            }
        }
        ref_ids.retain(|fid| !self.deleted.contains(fid));

        let mut rows: Vec<RefRow> = ref_ids
            .iter()
            .filter_map(|&fid| {
                let rec = if let Some(r) = self.doc.record(fid) {
                    r
                } else if let Ok(lo) = self.load_order() {
                    let (_, r) = nvre_world::fields::winner(lo, fid)?;
                    r
                } else {
                    return None;
                };
                let subs = rec.subrecords().ok()?;
                let placement = placement_in(&subs);
                Some(RefRow {
                    form_id: fid,
                    sig: rec.sig_str(),
                    edid: nvre_edit::document::edid_of(rec),
                    base: nvre_edit::document::value_of(&self.schema, rec, b"NAME")
                        .and_then(|v| v.as_str().map(str::to_string)),
                    persistent: self.index.refs.get(&fid).is_some_and(|e| e.persistent),
                    position: placement.map(|(p, _)| p),
                    rotation: placement.map(|(_, r)| r),
                    scale: scale_in(&subs),
                    owner: formid_in(&subs, b"XOWN"),
                    lock: lock_in(&subs),
                })
            })
            .collect();

        for p in self.doc.inserted() {
            if p.cell == cell {
                let rec = &p.record;
                if let Ok(subs) = rec.subrecords() {
                    let placement = placement_in(&subs);
                    rows.push(RefRow {
                        form_id: 0,
                        sig: rec.sig_str(),
                        edid: nvre_edit::document::edid_of(rec),
                        base: nvre_edit::document::value_of(&self.schema, rec, b"NAME")
                            .and_then(|v| v.as_str().map(str::to_string)),
                        persistent: false,
                        position: placement.map(|(p, _)| p),
                        rotation: placement.map(|(_, r)| r),
                        scale: scale_in(&subs),
                        owner: formid_in(&subs, b"XOWN"),
                        lock: lock_in(&subs),
                    });
                }
            }
        }
        rows
    }

    /// Open a cell for editing.
    pub fn open_cell(&mut self, cell: u32) {
        self.refs.clear();
        self.refs_cell = None;
        self.exterior_refs.clear();
        self.exterior_refs_key = None;
        // The table's resolved columns are per-cell too: a different cell's
        // owners are different records, so the old answers do not apply.
        self.details = RefDetails::default();
        self.details_cell = None;
        self.cell = Some(cell);
        self.selected = None;
        let name = self
            .index
            .cells
            .get(&cell)
            .and_then(|c| c.edid.clone())
            .unwrap_or_else(|| format!("{cell:08X}"));
        self.status = format!("opened {name}");
    }

    /// The next FormID a new record may use, in the plugin's own numbering.
    ///
    /// A FormID is file-local: its top byte indexes that file's own master
    /// list, and the value equal to the master count means "this file". So a
    /// new record's FormID has the plugin's own index in its top byte, and the
    /// low 24 bits are the next free ones.
    ///
    /// The low bits are what matter for a cell: an interior cell's block and
    /// sub-block labels are computed from them, and the save keeps them when
    /// it moves the cell into the new plugin's numbering.
    ///
    /// The counter starts at [`nvre_edit::save::first_object_id`], not at 1:
    /// offsets below `0x800` are the engine's hardcoded FormIDs, so a record
    /// given one would override an engine record rather than be a new one.
    pub fn next_local_form_id(&self) -> u32 {
        let self_index = self.doc.plugin().self_index() as u32;
        let base = self_index << 24;
        let mut next = nvre_edit::save::first_object_id(self_index as usize);
        for (fid, _) in self.doc.records() {
            if (fid >> 24) == self_index {
                next = next.max((fid & 0x00FF_FFFF) + 1);
            }
        }
        for c in self.doc.added_cells() {
            next = next.max((c.form_id & 0x00FF_FFFF) + 1);
        }
        for w in self.doc.added_worldspaces() {
            next = next.max((w.form_id & 0x00FF_FFFF) + 1);
        }
        base | next
    }

    /// A load-order FormID as one in the plugin being edited's own numbering.
    ///
    /// A worldspace reference is a load-order FormID, but a group label is a
    /// *plugin-local* one: the label names a record in the file the label is
    /// written to, so it has to be expressed in the numbering of the plugin
    /// being edited — which is what the save remaps.
    pub fn to_local_form_id(&self, global: u32) -> Option<u32> {
        // A worldspace this session created is already plugin-local: it was
        // handed out from the plugin's own range.
        if self
            .doc
            .added_worldspaces()
            .iter()
            .any(|x| x.form_id == global)
        {
            return Some(global);
        }
        let lo = self.load_order().ok()?;
        let source = lo
            .plugins
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(&self.doc.plugin().name))?;
        lo.to_local(source, global)
    }

    /// Create a new cell and stage it for the next save.
    ///
    /// The cell is staged rather than written immediately: a Bethesda plugin
    /// is written whole, and the cell's group tree depends on the FormID it
    /// will carry, which the save assigns.
    ///
    /// An interior cell has no worldspace and no grid. An exterior cell needs
    /// both: the worldspace it belongs to and the grid square it sits in.
    ///
    /// Returns the FormID the cell will carry.
    pub fn create_cell(
        &mut self,
        edid: &str,
        full: Option<String>,
        world: Option<u32>,
        grid: Option<(i32, i32)>,
        with_land: bool,
    ) -> Result<u32, String> {
        if edid.trim().is_empty() {
            return Err("a cell needs an Editor ID".into());
        }
        // An exterior cell has to name a worldspace that exists, or the cell
        // would hang off a worldspace the engine never finds. A worldspace
        // this session created counts: it is staged, and the save writes it
        // before the cell that hangs off it.
        let world =
            match world {
                None => None,
                Some(w) => {
                    let known = self.index.worldspaces.contains_key(&w)
                        || self.doc.added_worldspaces().iter().any(|x| x.form_id == w);
                    if !known {
                        return Err(format!("the worldspace {w:08X} is not in the load order"));
                    }
                    Some(self.to_local_form_id(w).ok_or_else(|| {
                        format!("the worldspace {w:08X} has no plugin-local FormID")
                    })?)
                }
            };
        let mut spec = nvre_edit::NewCell {
            edid: edid.to_string(),
            full,
            world,
            grid,
            with_land,
            data_flags: 0,
        };
        if world.is_none() {
            // An interior cell has no grid, and the game reads the grid of an
            // exterior cell to place it.
            spec.grid = None;
            spec.with_land = false;
        }
        let form_id = self.next_local_form_id();
        self.doc.add_cell(spec, form_id);
        self.status = format!("created cell {edid} ({form_id:08X})");
        Ok(form_id)
    }

    /// Create a new worldspace and stage it for the next save.
    ///
    /// Returns the FormID the worldspace will carry.
    pub fn create_worldspace(
        &mut self,
        edid: &str,
        full: Option<String>,
        default_land_height: f32,
        default_water_height: f32,
    ) -> Result<u32, String> {
        if edid.trim().is_empty() {
            return Err("a worldspace needs an Editor ID".into());
        }
        let spec = nvre_edit::NewWorldspace {
            edid: edid.to_string(),
            full,
            default_land_height,
            default_water_height,
            climate: None,
            water: None,
        };
        let form_id = self.next_local_form_id();
        self.doc.add_worldspace(spec, form_id);
        self.status = format!("created worldspace {edid} ({form_id:08X})");
        Ok(form_id)
    }

    /// The cells this session has staged for creation.
    pub fn staged_cells(&self) -> &[nvre_edit::document::PendingCell] {
        self.doc.added_cells()
    }

    /// The worldspaces this session has staged for creation.
    pub fn staged_worldspaces(&self) -> &[nvre_edit::document::PendingWorldspace] {
        self.doc.added_worldspaces()
    }

    /// Select a reference.
    pub fn select(&mut self, form_id: u32) {
        self.selected = Some(form_id);
    }

    /// Open any object by FormID and signature in its specialized editor window.
    pub fn open_object(&mut self, form_id: u32, sig: [u8; 4]) {
        self.editing_object = Some((form_id, sig));
        self.status = format!(
            "opened {} {:08X} in editor window",
            nvre_util::tag_str(&sig),
            form_id
        );
    }

    /// Close the object editor window.
    pub fn close_object_editor(&mut self) {
        self.editing_object = None;
    }

    /// Open a creature by FormID in the Creature Editor window.
    pub fn open_creature(&mut self, form_id: u32) {
        self.open_object(form_id, *b"CREA");
    }

    /// Close the Creature Editor window.
    pub fn close_creature(&mut self) {
        self.close_object_editor();
    }

    /// The selected reference's record.
    ///
    /// A newly placed record is staged with FormID 0 and lives in the session's
    /// pending inserts, not in the plugin, so it is read from there.
    /// The selected record, either staged, in the active plugin, or in the load order.
    pub fn selected_record(&self) -> Option<StagedRecord<'_>> {
        let fid = self.selected?;
        if fid == 0 {
            let p = self.doc.inserted().last()?;
            return Some(StagedRecord::Pending(&p.record));
        }
        if let Some(r) = self.doc.record(fid) {
            return Some(StagedRecord::Saved(r));
        }
        let lo = self.load_order().ok()?;
        let (_, r) = nvre_world::fields::winner(lo, fid)?;
        Some(StagedRecord::Saved(r))
    }

    /// A record as the mod will carry it, for any FormID.
    ///
    /// The single answer to "what is this record *now*", which is what every
    /// panel that shows a record should read. Three sources, in the order the
    /// mod itself resolves them:
    ///
    /// 1. A staged duplicate — a new record this session created, which no
    ///    plugin on disk holds yet.
    /// 2. The plugin being edited, with this session's edits applied — what a
    ///    record the mod defines itself will be written as.
    /// 3. The load order's winner, with the edits applied — what an override
    ///    of a master's record will be written as.
    ///
    /// The load order alone answers what the *masters* say, which is why the
    /// object window's list went stale the moment an edit was queued: it was
    /// reading source 3 and ignoring 1 and 2.
    ///
    /// Returns a clone rather than a borrow because the edits have to be
    /// applied to a copy; the callers are panels reading a handful of records,
    /// not the load-order walk that has to stay borrow-only.
    pub fn resolved_record(&self, form_id: u32) -> Option<nvre_esm::raw::Record> {
        let edits = self.doc.edits_for(form_id).unwrap_or(&[]);
        // A staged duplicate: a record this session created, which no plugin
        // on disk holds. Its own bytes, with the edits queued on it applied —
        // an edit made in the object editor after the duplicate was created is
        // keyed by the FormID the session handed out, and applying it here is
        // what makes it visible before the save.
        if let Some(p) = self
            .doc
            .inserted_top()
            .iter()
            .find(|p| p.record.header.form_id == form_id)
        {
            return Some(nvre_edit::document::resolved_record(&p.record, edits));
        }
        // The plugin being edited, or a master's record it overrides — either
        // way with the queued edits applied on top.
        if let Some(r) = self.doc.record(form_id) {
            return Some(nvre_edit::document::resolved_record(r, edits));
        }
        let lo = self.load_order().ok()?;
        let (_, r) = nvre_world::fields::winner(lo, form_id)?;
        Some(nvre_edit::document::resolved_record(r, edits))
    }

    /// The row the object window's list shows for a FormID, resolved.
    ///
    /// [`Self::resolved_record`] answers with a record; this answers with the
    /// three fields the table's columns are, resolved through the plugin's own
    /// string tables when it is localized. A `FULL` that is a string ID
    /// resolves to the name the player sees rather than to decoded ID bytes.
    pub fn resolved_object_row(&self, form_id: u32) -> Option<ObjectRow> {
        let rec = self.resolved_record(form_id)?;
        let localized = self
            .lo
            .plugins
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(self.doc.name()))
            .map(|p| p.header.is_localized())
            .unwrap_or(false);
        let strings = self.strings.for_plugin(self.doc.name(), localized);
        let (edid, name) = nvre_esm::inventory::names(&rec, strings).unwrap_or((None, None));
        Some(ObjectRow {
            fid: form_id,
            edid: edid.unwrap_or_default(),
            name: name.unwrap_or_default(),
        })
    }

    /// The signature of a record by FormID.
    pub fn record_sig(&self, form_id: u32) -> Option<[u8; 4]> {
        if let Some(r) = self.doc.record(form_id) {
            return Some(r.header.sig);
        }
        let lo = self.load_order().ok()?;
        let (_, r) = nvre_world::fields::winner(lo, form_id)?;
        Some(r.header.sig)
    }

    /// The usage counts, built once and rebuilt when the session's own records
    /// change.
    ///
    /// Both columns need a walk of the whole load order — the references table
    /// for the copies, every record's fields for the users — and the panel asks
    /// for them every frame, so they are cached the same way the object window's
    /// list is. `stale` is the invalidation: a write that adds a reference or a
    /// user has to show in the count, the way an edit to a record's Editor ID
    /// has to show in the list.
    pub fn usage(&mut self) -> &UsageCounts {
        if self.usage.is_none() || self.usage_stale {
            self.usage = Some(self.build_usage());
            self.usage_stale = false;
        }
        self.usage.as_ref().unwrap_or(&EMPTY_USAGE)
    }

    /// The two walks, in one pass over the load order.
    fn build_usage(&self) -> UsageCounts {
        let mut out = UsageCounts::default();
        let Ok(lo) = self.load_order() else {
            return out;
        };
        // Count: every reference in every cell, against the base object it
        // places. The world index already knows which reference is in which
        // cell, so this is a walk of the references rather than of the records.
        for (&fid, entry) in &self.index.refs {
            if !is_placed_sig(&entry.sig) {
                continue;
            }
            let Some(base) = self
                .resolved_record(fid)
                .and_then(|r| nvre_edit::document::value_of(&self.schema, &r, b"NAME"))
                .and_then(|v| v.as_str().map(str::to_string))
                .and_then(|b| u32::from_str_radix(b.strip_prefix("0x")?, 16).ok())
            else {
                continue;
            };
            *out.count.entry(base).or_insert(0) += 1;
        }
        // Users: every FormID field in every record, mapped to the load order.
        //
        // The walk reads the raw bytes at the positions the schema says hold a
        // FormID rather than decoding each subrecord: measured against
        // FalloutNV.esm, decoding all 2.3M subrecords costs 17.8s and reading
        // the same FormIDs costs 2.9s, with the same answer.
        for (pi, _plugin) in lo.plugins.iter().enumerate() {
            for r in &lo.records[pi] {
                let Ok(subs) = r.subrecords() else { continue };
                for sub in &subs {
                    let Some(def) = self.schema.lookup(&r.header.sig, &sub.sig) else {
                        continue;
                    };
                    if !type_has_formid(&self.schema, &def.ty) {
                        continue;
                    }
                    for f in scan_formids(&self.schema, &def.ty, &sub.data) {
                        if let Some(g) = lo.to_global(pi, f) {
                            *out.users.entry(g).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
        out
    }

    /// Queue a subrecord edit on the selected reference.
    pub fn edit_selected(&mut self, sig: &[u8; 4], data: Vec<u8>) {
        if let Some(fid) = self.selected {
            // A staged record has FormID 0 until the save; the edit has to reach
            // the staged copy, which is addressed by that same placeholder.
            self.doc.set_sub(fid, sig, data);
            // Every read that shows a record has to be re-read: the object
            // window's list, the object editor's grid and the property grid
            // all answer "what is this record" from a snapshot taken before
            // this edit is queued. The usage counts are the same: a field that
            // now holds a different FormID is a user of a different record.
            self.stale = true;
            self.usage_stale = true;
            self.status = format!("queued an edit on {fid:08X}");
        }
    }

    /// Set one field of one subrecord on the selected reference.
    ///
    /// `path` is the decoded field's path within the subrecord — `"level"` for a
    /// struct's own field, `"position.x"` for a nested one, and the empty string
    /// for a bare scalar subrecord whose value *is* the field. Those are the
    /// paths [`nvre_edit::encode::field_names`] returns, which is what the
    /// property grid addresses a row by.
    ///
    /// The whole subrecord is decoded, the field is replaced, and the result is
    /// re-encoded. That is the only safe route: a subrecord is a byte layout,
    /// and writing one field in place means re-deriving every other byte from
    /// the same decode, so the encoder — not the editor — decides what the
    /// field's neighbours look like.
    ///
    /// Returns `Err` with what went wrong rather than writing anything: a value
    /// the layout rejects (a `300` in a `u8`, a flag the schema does not name)
    /// is reported, and the queued edit is left alone.
    pub fn set_selected_field(
        &mut self,
        sig: &[u8; 4],
        path: &str,
        value: serde_json::Value,
    ) -> Result<(), String> {
        let Some(fid) = self.selected else {
            return Err("nothing selected".into());
        };
        // A staged record is not in the plugin yet, so its subrecords are read
        // from the pending copy rather than from the document.
        let rec_sig = {
            let Some(rec) = self.selected_record() else {
                return Err(format!("{fid:08X} not found"));
            };
            rec.record().header.sig
        };
        let subs = {
            let Some(rec) = self.selected_record() else {
                return Err(format!("{fid:08X} not found"));
            };
            rec.record().subrecords().map_err(|e| e.to_string())?
        };
        let Some(sub) = subs.iter().find(|s| &s.sig == sig) else {
            return Err(format!("{fid:08X} has no {}", nvre_util::tag_str(sig)));
        };
        // A signature may mean one thing in the record's own map and another
        // inside a block, so the block the subrecord falls in says which
        // meaning is meant, both for reading it and for writing it back.
        let block = nvre_esm::decode::block_of(&rec_sig, &subs, &self.schema, *sig);
        let mut decoded =
            nvre_edit::encode::decoded_in_block(&self.schema, &rec_sig, sub, block.as_deref())
                .ok_or_else(|| format!("{} does not decode", nvre_util::tag_str(sig)))?;
        set_path(&mut decoded, path, value)?;
        let bytes = nvre_edit::encode::encode_sub_len_in_block(
            &self.schema,
            &rec_sig,
            sig,
            &decoded,
            None,
            block.as_deref(),
        )
        .map_err(|e| e.to_string())?;
        // A staged record has FormID 0 until the save; the edit has to reach the
        // staged copy, which is addressed by that same placeholder.
        self.doc.set_sub(fid, sig, bytes);
        self.stale = true;
        self.usage_stale = true;
        self.status = format!("set {}.{} on {fid:08X}", nvre_util::tag_str(sig), path);
        Ok(())
    }

    /// Duplicate an object record under a new Editor ID.
    ///
    /// A duplicate is a **new record**, not an edit of the old one: the copy
    /// keeps every subrecord, and the `EDID` is replaced with the name the user
    /// typed. It is staged rather than written, so the save assigns it a
    /// FormID from the new plugin's own range and the copy's own subrecord
    /// FormIDs are remapped into the new plugin's numbering — a copy that kept
    /// the master's numbering would point at the master's records rather than
    /// at its own.
    ///
    /// Only a base object can be duplicated: a reference is placed in a cell
    /// and duplicated by placing it again.
    pub fn duplicate_object(
        &mut self,
        form_id: u32,
        sig: [u8; 4],
        edid: &str,
    ) -> Result<u32, String> {
        let edid = edid.trim();
        if edid.is_empty() {
            return Err("a duplicate needs an Editor ID".into());
        }
        let lo = self.load_order()?;
        let (_, orig) = nvre_world::fields::winner(lo, form_id)
            .ok_or_else(|| format!("{form_id:08X} is not in the load order"))?;
        if orig.header.sig != sig {
            return Err(format!(
                "{form_id:08X} is a {}, not a {}",
                orig.sig_str(),
                nvre_util::tag_str(&sig)
            ));
        }
        let mut subs = orig
            .subrecords()
            .map_err(|e| format!("{form_id:08X} will not parse: {e}"))?;
        // Replace the EDID, or add one where the record has none: a duplicate
        // is named by what the user typed, and an unnamed copy of a named
        // record is the one case that would collide.
        let encoded = nvre_edit::document::encode_zstring(edid);
        match subs.iter_mut().find(|s| &s.sig == b"EDID") {
            Some(s) => s.data = encoded,
            None => subs.insert(
                0,
                nvre_esm::raw::Subrecord {
                    sig: *b"EDID",
                    data: encoded,
                    extended: false,
                },
            ),
        }
        let payload = nvre_esm::raw::write_subrecords(&subs);
        let rec = nvre_esm::raw::Record {
            header: nvre_esm::raw::RecordHeader {
                sig,
                data_size: payload.len() as u32,
                flags: orig.header.flags,
                // 0 is the placeholder: the save assigns the real one from the
                // new plugin's own range.
                form_id: 0,
                vc_info: 0,
                form_version: orig.header.form_version,
                vc_info2: 0,
            },
            raw: payload,
            subrecords: Vec::new(),
            offset: 0,
        };
        let new = self.next_local_form_id();
        self.doc.insert_top(nvre_edit::document::PendingTopRecord {
            record: nvre_esm::raw::Record {
                header: nvre_esm::raw::RecordHeader {
                    // Staged with the session's own numbering, which the save
                    // moves into the plugin's; 0 would collide with every other
                    // staged record that has no ID yet.
                    form_id: new,
                    ..rec.header
                },
                ..rec
            },
        });
        self.status = format!("duplicated {form_id:08X} as {edid} ({new:08X})");
        // The list has to show the copy: it is a row that no plugin on disk
        // holds, so the only way it appears is through the resolver. The usage
        // counts follow, because the copy is a user of the record it was
        // copied from's fields, and its own copies count is zero.
        self.stale = true;
        self.usage_stale = true;
        Ok(new)
    }

    /// Queue a record for deletion by FormID.
    ///
    /// A row in the object window can be deleted straight from its context
    /// menu, which names the record rather than the selection — so the
    /// FormID comes in as an argument instead of being read off `selected`.
    /// Queuing a record that is already queued is a no-op, the way selecting
    /// an already-selected record is.
    pub fn delete_object(&mut self, form_id: u32) {
        if !self.deleted.contains(&form_id) {
            self.deleted.push(form_id);
        }
        self.doc.delete_record(form_id);
        // The object window's list shows the record as it was, which is a
        // row you can open and edit but will never see in-game. The usage
        // counts follow: a deleted record's copies and its users are gone
        // from the game, so the columns should not keep counting them.
        self.stale = true;
        self.usage_stale = true;
        self.status = format!("queued {form_id:08X} for deletion");
        self.refs_cell = None;
        self.exterior_refs_key = None;
    }

    /// Queue the selected reference for deletion.
    pub fn delete_selected(&mut self) {
        if let Some(fid) = self.selected.take() {
            self.delete_object(fid);
        }
    }

    /// Queued deletions, in the order asked.
    pub fn deleted(&self) -> &[u32] {
        &self.deleted
    }

    /// Place a new reference in the open cell at a game position.
    pub fn place(&mut self, base: u32, position: [f32; 3]) {
        let Some(cell) = self.cell else {
            self.status = "open a cell before placing".into();
            return;
        };
        // Staged with FormID 0: the save assigns the real one from the new
        // plugin's own range, and a placeholder here must not be mistaken for a
        // handed-out FormID. `placed` tracks it until then.
        let next = 0;
        let persistent = self
            .index
            .cells
            .get(&cell)
            .map(|c| c.world.is_some() && c.grid.is_none())
            .unwrap_or(false);
        let flags = if persistent {
            nvre_edit::document::REFR_PERSISTENT
        } else {
            0
        };
        let sig = self.record_sig(base).unwrap_or(*b"REFR");
        let placed_sig = match &sig {
            b"NPC_" => *b"ACHR",
            b"CREA" => *b"ACRE",
            _ => *b"REFR",
        };
        let rec = nvre_edit::document::make_placed_record(
            placed_sig,
            next,
            base,
            position,
            [0.0, 0.0, 0.0],
            None,
            None,
            flags,
        );
        self.doc
            .insert(nvre_edit::document::PendingRecord { cell, record: rec });
        self.selected = Some(next);
        self.status = format!("placed {base:08X} at {position:?}");
        self.refs_cell = None;
        self.exterior_refs_key = None;
    }

    /// Move the selected reference to a game position.
    pub fn move_selected(&mut self, position: [f32; 3]) {
        let Some((_, rot, _)) = self.selected_placement() else {
            return;
        };
        let mut data = vec![0u8; 24];
        for i in 0..3 {
            data[i * 4..i * 4 + 4].copy_from_slice(&position[i].to_le_bytes());
            data[(3 + i) * 4..(3 + i) * 4 + 4].copy_from_slice(&rot[i].to_le_bytes());
        }
        self.edit_selected(b"DATA", data);
        self.refs_cell = None;
        self.exterior_refs_key = None;
    }

    /// Rotate the selected reference in game space (Euler angles [pitch, roll, yaw] in radians).
    pub fn rotate_selected(&mut self, rotation: [f32; 3]) {
        let Some((pos, _, _)) = self.selected_placement() else {
            return;
        };
        let mut data = vec![0u8; 24];
        for i in 0..3 {
            data[i * 4..i * 4 + 4].copy_from_slice(&pos[i].to_le_bytes());
            data[(3 + i) * 4..(3 + i) * 4 + 4].copy_from_slice(&rotation[i].to_le_bytes());
        }
        self.edit_selected(b"DATA", data);
        self.refs_cell = None;
        self.exterior_refs_key = None;
    }

    /// Scale the selected reference (uniform scale).
    pub fn scale_selected(&mut self, scale: f32) {
        let bytes = scale.to_le_bytes().to_vec();
        self.edit_selected(b"XSCL", bytes);
        self.refs_cell = None;
        self.exterior_refs_key = None;
    }

    /// The selected reference's current (position, rotation, scale) in game space.
    pub fn selected_placement(&self) -> Option<([f32; 3], [f32; 3], f32)> {
        let fid = self.selected?;
        let rec = self.selected_record()?;
        let subs = rec.record().subrecords().ok()?;
        let (mut pos, mut rot) = placement_in(&subs)?;
        let mut scale = scale_in(&subs).unwrap_or(1.0);

        if let Some(edits) = self.doc.edits_for(fid) {
            for e in edits {
                match e {
                    nvre_edit::document::Edit::SetSub { sig, data }
                        if sig == b"DATA" && data.len() >= 24 =>
                    {
                        let f = |i: usize| -> Option<f32> {
                            Some(f32::from_le_bytes(
                                data.get(i * 4..i * 4 + 4)?.try_into().ok()?,
                            ))
                        };
                        if let (Some(x), Some(y), Some(z), Some(rx), Some(ry), Some(rz)) =
                            (f(0), f(1), f(2), f(3), f(4), f(5))
                        {
                            pos = [x, y, z];
                            rot = [rx, ry, rz];
                        }
                    }
                    nvre_edit::document::Edit::SetSub { sig, data }
                        if sig == b"XSCL" && data.len() >= 4 =>
                    {
                        if let Ok(b) = data[..4].try_into() {
                            scale = f32::from_le_bytes(b);
                        }
                    }
                    _ => {}
                }
            }
        }
        Some((pos, rot, scale))
    }

    /// Nudge the selected reference by a game-space offset.
    ///
    /// The viewport's gizmo will drive this once it can drag; for now the
    /// property panel's move fields call it, which is the same code path.
    pub fn nudge_selected(&mut self, delta: [f32; 3]) {
        let Some(rec) = self.selected_record() else {
            return;
        };
        let Some(p) = position_of(rec.record()) else {
            return;
        };
        self.move_selected([p[0] + delta[0], p[1] + delta[1], p[2] + delta[2]]);
    }

    /// The FormID the selected record will have once saved.
    ///
    /// A newly placed record is staged with FormID 0 and the save assigns its real
    /// one from the new plugin's own range, so the selection has to be translated
    /// before it names anything in the saved file.
    pub fn selected_saved_id(&self) -> Option<u32> {
        let fid = self.selected?;
        if fid != 0 {
            return Some(fid);
        }
        let base = (self.masters.len() as u32) << 24;
        Some(base + self.placed_index as u32)
    }

    /// The pending edits, for a save summary.
    pub fn pending_edits(&self) -> usize {
        self.doc.edits().map(|(_, e)| e.len()).sum::<usize>()
            + self.doc.inserted().len()
            + self.deleted.len()
    }

    /// Where a save goes: the plugin being edited's own name, in the game's Data
    /// folder.
    ///
    /// The folder is the session's, not the process's working directory: a bare
    /// file name would be resolved against wherever the editor was launched from,
    /// and the mod would be written outside the game's load order. The name is the
    /// document's, so a plugin saved under a new name is saved under that name.
    pub fn save_target(&self) -> std::path::PathBuf {
        self.data_dir.join(self.doc.name())
    }

    /// Save as a new plugin.
    ///
    /// The session's own author and description go into the header, so a plugin
    /// created with them keeps them across every save rather than only the
    /// first.
    pub fn save(&self, out: &std::path::Path) -> Result<nvre_edit::SaveReport, String> {
        let mut masters = nvre_edit::MasterList::new(self.masters.clone());
        masters.author = self.author.clone();
        masters.description = self.description.clone();
        nvre_edit::write_plugin_with_load_order(
            &self.doc,
            &masters,
            &self.schema,
            Some(&self.lo),
            out,
            nvre_edit::save::Scope::OwnRecords,
        )
    }
}

/// A cell, for the browser table.
#[derive(Debug, Clone)]
pub struct CellRow {
    pub form_id: u32,
    pub edid: Option<String>,
    /// The cell's display name (CELL FULL), when it has one.
    pub name: Option<String>,
    pub interior: bool,
    pub refs: usize,
    pub world: bool,
    /// The cell's grid coordinate (CELL XCLC) for an exterior cell; `None` for
    /// an interior.
    pub grid: Option<(i32, i32)>,
}

impl CellRow {
    /// The row's Editor ID column: the EDID, or the FormID without one.
    pub fn editor_id(&self) -> String {
        self.edid
            .clone()
            .unwrap_or_else(|| format!("{:08X}", self.form_id))
    }

    /// The row's Location column.
    ///
    /// An exterior cell's place in the world is its grid square, which is what
    /// GECK shows; an interior belongs to no world, so it says so rather than
    /// leaving the column blank.
    pub fn location(&self) -> String {
        match self.grid {
            Some((x, y)) => format!("{x}, {y}"),
            None => "interior".to_string(),
        }
    }
}

/// A worldspace, for the cell browser's dropdown.
#[derive(Debug, Clone)]
pub struct WorldspaceRow {
    pub form_id: u32,
    /// What the dropdown shows: the Editor ID, or the display name without one.
    pub label: String,
    /// The in-game name (CELL FULL), when the worldspace has one.
    pub name: Option<String>,
}

/// Which cells the browser lists: the interiors, or one worldspace's cells.
///
/// This is the cell window's dropdown state, in the same shape GECK's Cell View
/// uses — "Interior" plus one entry per worldspace, with the list below it
/// filtered to whichever is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellScope {
    /// Cells that belong to no worldspace: every interior.
    #[default]
    Interiors,
    /// One worldspace's exterior cells, by WRLD FormID.
    Worldspace(u32),
}

impl CellScope {
    /// The dropdown's label for this scope.
    pub fn label(self, worldspaces: &[WorldspaceRow]) -> String {
        match self {
            CellScope::Interiors => "Interiors".to_string(),
            CellScope::Worldspace(w) => worldspaces
                .iter()
                .find(|r| r.form_id == w)
                .map(|r| r.label.clone())
                .unwrap_or_else(|| format!("{w:08X}")),
        }
    }
}

/// A cell's own lighting, as the renderer draws it.
///
/// This is the `CELL`'s `XCLL` subrecord — the ambient colour, the directional
/// (sun) colour, and the fog. Every game's interiors carry one; measured on the
/// shipped masters, an exterior cell carries none (it takes its lighting from
/// the weather system instead), so this is an interior's lighting.
///
/// ## The two lengths
///
/// | Game | `XCLL` |
/// |---|---|
/// | FNV, FO3 | 40 B — exactly this struct |
/// | Skyrim LE, Skyrim SE, FO4 | 92 B — this struct, then 52 more bytes |
///
/// The 40-byte prefix is the same in all five. Skyrim LE/SE and FO4 store
/// 52 additional bytes: a 6-axis directional ambient cube (X+, X-, Y+, Y-, Z+, Z-),
/// specular colour, directional scale, fog far colour, fog max, and light fade distances.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellLighting {
    /// The ambient colour, as 0-255 bytes.
    pub ambient: [u8; 3],
    /// The directional (sun) colour, as 0-255 bytes.
    pub directional: [u8; 3],
    /// The fog colour, as 0-255 bytes.
    pub fog: [u8; 3],
    /// Where the fog starts, in game units. Zero means no fog at all.
    pub fog_near: f32,
    /// Where the fog is fully opaque, in game units.
    pub fog_far: f32,
    /// The sun's heading around the vertical, in degrees.
    pub rotation_xy: i32,
    /// The sun's elevation, in degrees.
    pub rotation_z: i32,
    /// The directional light's fade, a scale on its colour.
    pub directional_fade: f32,
    /// The distance beyond which nothing is drawn for fog.
    pub fog_clip_dist: f32,
    /// The fog's power: how sharply it thickens with distance.
    pub fog_power: f32,
    /// 6-axis directional ambient colors (Skyrim LE/SE, FO4 92-byte XCLL or LGTM DALC):
    /// [X+, X-, Y+, Y-, Z+, Z-], each 3 bytes.
    pub directional_ambient: Option<[[u8; 3]; 6]>,
    /// Specular color for directional ambient (Skyrim LE/SE, FO4).
    pub specular: Option<[u8; 3]>,
    /// Directional ambient scale multiplier (Skyrim LE/SE, FO4).
    pub directional_scale: Option<f32>,
}

impl CellLighting {
    /// Whether the cell draws any fog at all.
    ///
    /// A `fog_far` of zero is what a cell with no fog stores: there is no
    /// distance at which the fog is opaque, so there is nothing to blend to.
    /// Many cells do this deliberately.
    pub fn has_fog(&self) -> bool {
        self.fog_far > 0.0 && self.fog_far > self.fog_near
    }

    /// The ambient colour as a Bevy colour, bytes scaled linearly to 0-1.
    ///
    /// In Bethesda engines (Gamebryo and pre-FO4 PBR Creation Engine), ambient
    /// and directional lights are direct un-curved linear multipliers on diffuse textures.
    /// `Color::LinearRgba` avoids double-gamma darkening in the PBR lighting pass.
    pub fn ambient_color(&self) -> bevy::color::Color {
        if let (Some(dalc), scale_opt) = (self.directional_ambient, self.directional_scale) {
            let scale = scale_opt.unwrap_or(1.0).max(0.0);
            let mut sum_r = f32::from(self.ambient[0]);
            let mut sum_g = f32::from(self.ambient[1]);
            let mut sum_b = f32::from(self.ambient[2]);
            for axis in &dalc {
                sum_r += f32::from(axis[0]) * scale;
                sum_g += f32::from(axis[1]) * scale;
                sum_b += f32::from(axis[2]) * scale;
            }
            // 1 base ambient + 6 directional axes = 7 components
            let avg = [
                ((sum_r / 7.0).clamp(0.0, 255.0)) as u8,
                ((sum_g / 7.0).clamp(0.0, 255.0)) as u8,
                ((sum_b / 7.0).clamp(0.0, 255.0)) as u8,
            ];
            bytes_color(avg)
        } else {
            bytes_color(self.ambient)
        }
    }

    /// The directional colour as a Bevy colour, bytes scaled linearly to 0-1.
    pub fn directional_color(&self) -> bevy::color::Color {
        bytes_color(self.directional)
    }

    /// The fog colour as a Bevy colour, bytes scaled to 0-1.
    pub fn fog_color(&self) -> bevy::color::Color {
        // DistanceFog in Bevy works as an sRGB display color
        bevy::color::Color::srgb(
            f32::from(self.fog[0]) / 255.0,
            f32::from(self.fog[1]) / 255.0,
            f32::from(self.fog[2]) / 255.0,
        )
    }
}

/// A byte triple as a Bevy linear color without double-gamma darkening.
fn bytes_color(b: [u8; 3]) -> bevy::color::Color {
    bevy::color::Color::LinearRgba(bevy::color::LinearRgba::new(
        f32::from(b[0]) / 255.0,
        f32::from(b[1]) / 255.0,
        f32::from(b[2]) / 255.0,
        1.0,
    ))
}

/// Read a cell's `XCLL` bytes into [`CellLighting`].
///
/// `None` unless the subrecord is at least the 40 bytes the layout needs.
/// If 92 bytes or more are present, also decodes the Skyrim/FO4 directional ambient cube.
pub fn cell_lighting(bytes: &[u8]) -> Option<CellLighting> {
    if bytes.len() < 40 {
        return None;
    }
    let f32_at =
        |o: usize| f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    let i32_at =
        |o: usize| i32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);

    let (directional_ambient, specular, directional_scale) = if bytes.len() >= 92 {
        // Bytes 40..64: 6 directional ambient colours (X+, X-, Y+, Y-, Z+, Z-), 4 bytes each (RGB + 1 unused pad byte)
        let dalc = [
            [bytes[40], bytes[41], bytes[42]],
            [bytes[44], bytes[45], bytes[46]],
            [bytes[48], bytes[49], bytes[50]],
            [bytes[52], bytes[53], bytes[54]],
            [bytes[56], bytes[57], bytes[58]],
            [bytes[60], bytes[61], bytes[62]],
        ];
        // Bytes 64..68: specular RGB + 1 pad byte
        let spec = [bytes[64], bytes[65], bytes[66]];
        // Bytes 68..72: scale (f32)
        let scale = f32_at(68);
        (Some(dalc), Some(spec), Some(scale))
    } else {
        (None, None, None)
    };

    Some(CellLighting {
        ambient: [bytes[0], bytes[1], bytes[2]],
        directional: [bytes[4], bytes[5], bytes[6]],
        fog: [bytes[8], bytes[9], bytes[10]],
        fog_near: f32_at(12),
        fog_far: f32_at(16),
        rotation_xy: i32_at(20),
        rotation_z: i32_at(24),
        directional_fade: f32_at(28),
        fog_clip_dist: f32_at(32),
        fog_power: f32_at(36),
        directional_ambient,
        specular,
        directional_scale,
    })
}

/// A placed light's own data, for the viewport to draw a real light from.
///
/// This is the `LIGH` record's `DATA` as the game stores it, read once per base
/// object and cached. The viewport turns it into a Bevy `PointLight` or
/// `SpotLight` at the reference's position.
///
/// ## The two families name the same fields differently
///
/// Measured on the shipped masters:
///
/// | Game | `DATA` | Colour | Radius | FOV | Spot flag | Off-by-default |
/// |---|---|---|---|---|---|---|
/// | FNV | 32 B | `color` | `radius` | `fov` | `Spot Light` | `Off By Default` |
/// | FO3 | 32 B | `color` | `radius` | `fov` | `Spot Light` | `Off By Default` |
/// | Skyrim LE | 48 B | *not decoded* | — | — | — | — |
/// | Skyrim SE | 64 B | `Color` | `Radius` | `FOV` | `NonShadow Spotlight` | `Off By Default` |
/// | FO4 | 64 B | `Color` | `Radius` | `FOV` | `NonShadow Spotlight` | `Off By Default` |
///
/// Fallout's 32-byte layout is one struct; Skyrim SE and Fallout 4 share a
/// 64-byte one with capitalised field names. The reader takes each field by both
/// spellings, which is what lets one function cover the four games that decode.
#[derive(Debug, Clone, PartialEq)]
pub struct LightData {
    /// The light's colour, as 0-255 bytes.
    pub color: [u8; 3],
    /// The light's radius in game units.
    pub radius: f32,
    /// The light's fade value: its brightness multiplier, as the game stores it.
    ///
    /// `FNAM`. The engine scales a light's intensity by it, so 2.0 is twice as
    /// bright and 0.1 is a tenth. The engine's own default is 1.0, and a light
    /// with no `FNAM` at all is drawn at 1.0 too — xEdit writes one when it
    /// opens a record that lacks it.
    pub fade: f32,
    /// The spotlight's field of view in degrees. A point light ignores it.
    pub fov: f32,
    /// The light is a spotlight rather than an omnidirectional one.
    pub spot: bool,
    /// The light is off until something turns it on.
    ///
    /// The game's `Off By Default` flag. The viewport honours it by simply not
    /// drawing the light: a light that is off produces nothing, which is what
    /// the flag asks for.
    pub off_by_default: bool,
}

impl LightData {
    /// A light with no data: a point light of radius one, white, full fade.
    ///
    /// The fallback for a game whose `LIGH` layout is not decoded. It is not a
    /// real light — it is a marker that the reference *is* a light, so a cell
    /// full of lamps does not silently render as if it had none.
    pub const UNKNOWN: LightData = LightData {
        color: [255, 255, 255],
        radius: 1.0,
        fade: 1.0,
        fov: 90.0,
        spot: false,
        off_by_default: false,
    };

    /// The light's colour as a Bevy colour, bytes scaled to 0-1.
    pub fn bevy_color(&self) -> bevy::color::Color {
        bevy::color::Color::srgb(
            f32::from(self.color[0]) / 255.0,
            f32::from(self.color[1]) / 255.0,
            f32::from(self.color[2]) / 255.0,
        )
    }
}

/// Read a `LIGH` record's `DATA` into [`LightData`].
///
/// `None` means the record has no `DATA` at all, which is not a light that
/// failed to read but a record that carries no light data.
fn light_data_in(rec: &nvre_esm::Record, schema: &Schema) -> Option<LightData> {
    let subs = rec.subrecords().ok()?;
    let data = subs
        .iter()
        .find(|s| s.sig.as_slice() == b"DATA")
        .and_then(|s| nvre_edit::encode::decoded(schema, b"LIGH", s))?;
    let mut out = light_data(&data);
    // The fade is a sibling subrecord, not part of `DATA`: `FNAM` is the
    // brightness multiplier, and the engine's default is 1.0 when it is absent.
    if let Some(fnam) = subs
        .iter()
        .find(|s| s.sig.as_slice() == b"FNAM")
        .and_then(|s| nvre_edit::encode::decoded(schema, b"LIGH", s))
        .and_then(|v| v.as_f64())
    {
        out.fade = fnam as f32;
    }
    Some(out)
}

/// Read a `LIGH` record's `DATA`, taking each field by either spelling.
///
/// The two families differ in more than capitalisation: Fallout's `DATA` is
/// 32 bytes and Skyrim SE's is 64, with extra fields (`Constant`, `Exponent`,
/// `Scalar`, `Flicker Effect`, `God Rays`) interleaved. Reading by field name
/// rather than by offset is what makes one function cover both, and it is the
/// same trick `shape_props` uses for the NIF's shader properties.
///
/// A field that is absent keeps its default: a missing radius is 1.0, a
/// missing colour is white. That is deliberate — a light with no colour is not
/// a light the engine would draw, but a light the viewport cannot read should
/// still be visible rather than silently dropped.
pub fn light_data(value: &serde_json::Value) -> LightData {
    let mut out = LightData::UNKNOWN;
    let Some(obj) = value.as_object() else {
        return out;
    };
    // The colour: `color` (Fallout) or `Color` (Skyrim SE, Fallout 4). Each is
    // a struct of bytes, and each family spells its components differently too.
    for key in ["color", "Color"] {
        if let Some(c) = obj.get(key) {
            out.color = [
                byte_field(c, &["red", "Red"]).unwrap_or(255),
                byte_field(c, &["green", "Green"]).unwrap_or(255),
                byte_field(c, &["blue", "Blue"]).unwrap_or(255),
            ];
        }
    }
    // The radius and the field of view, both families.
    for key in ["radius", "Radius"] {
        if let Some(n) = num_field(obj, key) {
            out.radius = n;
        }
    }
    for key in ["fov", "FOV"] {
        if let Some(n) = num_field(obj, key) {
            out.fov = n;
        }
    }
    // The spot flag. The two families call it differently and at different
    // bits: Fallout's is `Spot Light` at bit 9, Skyrim's is `Spot Light` at
    // bit 9 too — plus `Shadow Spotlight` at bit 10 and `NonShadow Spotlight`
    // at bit 15 in Fallout 4. A shadow flag is about the shadows, not the
    // shape: an omnidirectional light casts shadows too. `Shadow Hemisphere`
    // (bit 11), `Shadow Omnidirectional` (bit 12) and `Portal-strict` (bit 13)
    // are never cones, so they are not read.
    out.spot = flag_field(
        obj,
        &["Spot Light", "Shadow Spotlight", "NonShadow Spotlight"],
    );
    out.off_by_default = flag_field(obj, &["Off By Default"]);
    out
}

/// A byte component of a decoded colour struct.
fn byte_field(v: &serde_json::Value, names: &[&str]) -> Option<u8> {
    let o = v.as_object()?;
    let n = names.iter().find_map(|n| o.get(*n))?.as_u64()?;
    u8::try_from(n).ok()
}

/// A number field of a decoded `DATA`, by either spelling.
fn num_field(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<f32> {
    obj.get(key)?.as_f64().map(|n| n as f32)
}

/// A flag field of a decoded `DATA`.
///
/// The schema decodes a flags subrecord as `{"value": n, "fields": {name: 0|1}}`
/// — the per-bit values are **numbers**, not booleans, so a `0`/`1` is read
/// either way rather than with `as_bool`, which would never match and silently
/// fall through to the bit-number table.
///
/// The named table is read first; when it has no entry for the name, the raw
/// value is masked against the bit the name sits at, which is the fallback for
/// a schema that names no bits.
fn flag_field(obj: &serde_json::Map<String, serde_json::Value>, names: &[&str]) -> bool {
    let Some(f) = obj.get("flags").or_else(|| obj.get("Flags")) else {
        return false;
    };
    for name in names {
        // The named table first: the schema's own bit table is authoritative.
        if let Some(v) = f.get("fields").and_then(|t| t.get(*name)) {
            if v.as_u64().unwrap_or(0) != 0 || v.as_bool().unwrap_or(false) {
                return true;
            }
            continue;
        }
        // A name the schema did not name: mask the raw value against the bit
        // the name sits at. A name may sit at several bits across the games, so
        // the first match wins — but a bit that is *not* set must not stop the
        // search, or a later name would never be tried.
        if let (Some(bit), Some(raw)) = (flag_bit(name), f.get("value").and_then(|v| v.as_u64())) {
            if raw & (1 << bit) != 0 {
                return true;
            }
        }
    }
    false
}

/// The bit a named flag sits at, for the flags the schema does not name.
///
/// The table is the union of the layouts, because the same name means a
/// different bit in different games: Fallout's `Spot Light` is bit 9, Skyrim's
/// is bit 9 too, but `Shadow Spotlight` is bit 10 in Fallout and bit 10 in
/// Skyrim, and Fallout 4's `NonShadow Spotlight` is bit 15. A name that appears
/// at several bits returns the first, so a game whose schema does not name the
/// bit still gets an answer rather than a silent false.
///
/// The bits are the mask values from xEdit's `wbDefinitionsTES5.pas` and
/// `wbDefinitionsFNV.pas` (`{0x00000200}` is bit 9, not 10).
///
/// This is a fallback for a schema that names no bits. When the schema names
/// them — which the `skyrim`, `fnv` and `fo4` layers all do — the named table
/// is authoritative and this is not consulted.
fn flag_bit(name: &str) -> Option<u32> {
    const TABLE: &[(&str, u32)] = &[
        ("Off By Default", 5),
        // Fallout's family: bit 9 spot, bit 10 spot shadow.
        ("Spot Light", 9),
        ("Spot Shadow", 10),
        // Skyrim's family: bit 9 spot, 10 shadow spot, 11 hemisphere,
        // 12 omnidirectional, 13 portal-strict.
        ("Shadow Spotlight", 10),
        ("Shadow Hemisphere", 11),
        ("Shadow Omnidirectional", 12),
        ("Portal-strict", 13),
        // Fallout 4's: bit 15 non-shadow spot.
        ("NonShadow Spotlight", 15),
    ];
    TABLE.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

/// A placed reference, for the browser list and the viewport.
#[derive(Debug, Clone)]
pub struct RefRow {
    pub form_id: u32,
    pub sig: String,
    pub edid: Option<String>,
    pub base: Option<String>,
    pub persistent: bool,
    pub position: Option<[f32; 3]>,
    /// The reference's rotation, in radians, as the file stores it. Without it a
    /// rotated object — a door, a piece of furniture, a road — is drawn
    /// axis-aligned, which is most of what "positioned wrong" looks like.
    pub rotation: Option<[f32; 3]>,
    /// The reference's scale from its XSCL subrecord, when present (default 1.0).
    pub scale: Option<f32>,
    /// XOWN: who the reference belongs to, as a load-order FormID.
    ///
    /// The FormID rather than a resolved name: the table resolves it once per
    /// owner through the session's cache, and a raw FormID is what a test can
    /// check against the file's own bytes.
    pub owner: Option<u32>,
    /// XLOC: the lock on the reference, when it carries one.
    pub lock: Option<LockRow>,
}

/// A reference's lock, from its XLOC subrecord.
///
/// Only doors and containers carry one in the shipped masters [observed: 426
/// REFR XLOC in FalloutNV.esm], and the level is what the lockpick menu opens
/// with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockRow {
    /// 0 Very Easy, 25 Easy, 50 Average, 75 Hard, 100 Very Hard, 255 Requires
    /// Key [documented: GECK wiki GetLockLevel].
    pub level: u8,
    /// The key that opens it, as a load-order FormID, when one is named.
    pub key: Option<u32>,
    /// XLOC flag bit 2, "Leveled Lock": the level is chosen by the levelled
    /// item rather than fixed [documented: xEdit REFR XLOC].
    pub leveled: bool,
}

impl LockRow {
    /// The lock's level as the game reports it: a number for a pickable lock,
    /// "key" when only the key opens it.
    pub fn label(self) -> String {
        if self.level == 255 {
            return "key".to_string();
        }
        format!("{}", self.level)
    }
}

/// The references table's resolved columns, for one cell.
///
/// An owner's name and a base object's record type both live behind the load
/// order, which is the editor's most expensive query. Both are resolved once per
/// cell and keyed by FormID, so a row's lookups are a hash map read: the table
/// draws every frame, and resolving per row per frame is what made the place
/// palette cost half a frame per second (see [`Session::palette`]).
#[derive(Debug, Clone, Default)]
pub struct RefDetails {
    /// Owner FormID → the name the table shows.
    pub owners: HashMap<u32, String>,
    /// Base FormID → what the base object is, as GECK names it: "Static",
    /// "Activator", "Misc. Item".
    ///
    /// The schema's own name for the base's record type, which is what GECK's
    /// render window shows rather than the four-letter signature. No two record
    /// types share a name [observed: all 107 in `schemas/fnv`], so it also
    /// serves as the type dropdown's identity.
    pub types: HashMap<u32, String>,
    /// Base FormID → the base object's Editor ID.
    ///
    /// This is what the Editor ID column shows for a placed object. A
    /// reference's own EDID is the exception rather than the rule — 5,011 of
    /// FalloutNV's 307,710 REFRs carry one [observed: `fnvtool esm survey`] —
    /// so the column falls back to the name of the object being placed, which
    /// is what GECK shows.
    pub bases: HashMap<u32, String>,
}

impl RefDetails {
    /// What an owner's FormID names, for one row.
    pub fn owner(&self, owner: u32) -> Option<&str> {
        self.owners.get(&owner).map(String::as_str)
    }

    /// What a base FormID's record type is, for one row.
    pub fn base_type(&self, base: u32) -> Option<&str> {
        self.types.get(&base).map(String::as_str)
    }

    /// What a base FormID's Editor ID is, for one row.
    pub fn base_edid(&self, base: u32) -> Option<&str> {
        self.bases.get(&base).map(String::as_str)
    }
}

/// The details of a cell that is not open: empty, so a table with no cell has
/// no columns to resolve either.
static EMPTY_DETAILS: std::sync::LazyLock<RefDetails> =
    std::sync::LazyLock::new(RefDetails::default);

/// Set the field at `path` inside a decoded subrecord value, in place.
///
/// `path` is dot-separated (`"position.x"`) and indexes into the decoded JSON
/// object, so it works for any nesting the schema produces. The empty path
/// replaces the whole value, which is how a bare scalar subrecord — `XOWN`,
/// `XSCL` — is addressed: it has no inner fields, so the subrecord *is* the
/// field.
///
/// A path that names no existing field is refused: creating one would add a key
/// the layout does not declare, which the encoder rejects anyway — better to say
/// so here, where the message can name the field.
fn set_path(
    value: &mut serde_json::Value,
    path: &str,
    new: serde_json::Value,
) -> Result<(), String> {
    // A bare scalar subrecord is addressed by the empty path: it has no inner
    // fields, so the subrecord's own value is the field.
    if path.is_empty() {
        *value = new;
        return Ok(());
    }
    let mut at = value;
    let mut parts = path.split('.').peekable();
    while let Some(part) = parts.next() {
        let last = parts.peek().is_none();
        let obj = at
            .as_object_mut()
            .ok_or_else(|| format!("{part} is not a field"))?;
        if last {
            if !obj.contains_key(part) {
                return Err(format!("no field {part:?} in this subrecord"));
            }
            obj.insert(part.to_string(), new);
            return Ok(());
        }
        at = obj
            .get_mut(part)
            .ok_or_else(|| format!("no field {part:?} in this subrecord"))?;
    }
    Err("empty field path".into())
}

/// A reference's position and rotation from its DATA subrecord.
///
/// DATA is 24 bytes: position then rotation, both xyz f32, in game units and
/// radians. The rotation is what makes a door swing the right way and a road run
/// the right direction, so it is read alongside the position rather than left
/// for the caller to find.
pub fn placement_of(rec: &nvre_esm::Record) -> Option<([f32; 3], [f32; 3])> {
    placement_in(&rec.subrecords().ok()?)
}

/// [`placement_of`] over subrecords that are already parsed.
fn placement_in(subs: &[nvre_esm::Subrecord]) -> Option<([f32; 3], [f32; 3])> {
    let data = subs.iter().find(|s| &s.sig == b"DATA")?;
    if data.data.len() < 24 {
        return None;
    }
    let f = |i: usize| -> Option<f32> {
        Some(f32::from_le_bytes(
            data.data.get(i * 4..i * 4 + 4)?.try_into().ok()?,
        ))
    };
    Some(([f(0)?, f(1)?, f(2)?], [f(3)?, f(4)?, f(5)?]))
}

/// A reference's position from its DATA subrecord, in game units.
pub fn position_of(rec: &nvre_esm::Record) -> Option<[f32; 3]> {
    placement_of(rec).map(|(p, _)| p)
}

/// A FormID-valued subrecord, as the file stores it.
///
/// The value is file-local: mapping it to the load order needs the plugin's
/// master list, so a caller that shows the FormID resolves it once per owner
/// rather than once per reference (see [`Session::ref_details`]).
///
/// 0 is the null reference and no owner at all, so it reads as `None` rather
/// than as the master's own range.
fn formid_in(subs: &[nvre_esm::Subrecord], sig: &[u8; 4]) -> Option<u32> {
    let s = subs.iter().find(|s| &s.sig == sig)?;
    let local = <[u8; 4]>::try_from(s.data.get(..4)?)
        .ok()
        .map(u32::from_le_bytes)?;
    (local != 0).then_some(local)
}

/// A reference's scale, from its XSCL subrecord.
///
/// Defaults to 1.0 when XSCL is absent. When present, XSCL is a 4-byte little-endian float.
pub fn scale_in(subs: &[nvre_esm::Subrecord]) -> Option<f32> {
    let s = subs.iter().find(|s| &s.sig == b"XSCL")?;
    let bytes: [u8; 4] = s.data.get(..4)?.try_into().ok()?;
    Some(f32::from_le_bytes(bytes))
}

/// A reference's lock, from its XLOC subrecord.
///
/// XLOC is 20 bytes [observed: 420 of the 426 in FalloutNV.esm]: level u8, three
/// unused bytes, key FormID, flags u8, three unused bytes. The six 12-byte ones
/// carry no key [observed: same survey] — the key is read as far as the data
/// goes, so both lengths decode.
fn lock_in(subs: &[nvre_esm::Subrecord]) -> Option<LockRow> {
    let d = &subs.iter().find(|s| &s.sig == b"XLOC")?.data;
    let level = *d.first()?;
    let key = d
        .get(4..8)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map(u32::from_le_bytes)
        .filter(|&k| k != 0);
    let flags = d.get(8).copied().unwrap_or(0);
    Some(LockRow {
        level,
        key,
        // Bit 2, "Leveled Lock" [documented: xEdit REFR XLOC].
        leveled: flags & 0x4 != 0,
    })
}

/// One category of the object window: GECK's Object Window groups records into
/// top-level categories ("WorldObjects", "Items", "Actors", ...) and subcategories
/// named after the record type ("Static", "Weapon", "Creature", ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectGroup {
    /// Top-level GECK group: "Actors", "Items", "Magic", "Special Effects",
    /// "WorldObjects", "Audio", "Miscellaneous".
    pub group: String,
    /// The category's name: "Static", "Container", "Moveable Static".
    pub category: String,
    /// The record signature: `*b"STAT"`, etc.
    pub sig: [u8; 4],
    /// The category's objects, sorted by name.
    pub objects: Vec<ObjectRow>,
}

/// One object in the window's list: its FormID and the fields the list's
/// table shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRow {
    /// The base object's global FormID.
    pub fid: u32,
    /// The Editor ID, empty when the record has none.
    pub edid: String,
    /// The full name, empty when the record has none.
    pub name: String,
}

impl ObjectRow {
    /// What the list sorts and filters by, and what a single-label list
    /// would have shown: the name where there is one, the Editor ID
    /// otherwise.
    pub fn label(&self) -> &str {
        if self.name.is_empty() {
            &self.edid
        } else {
            &self.name
        }
    }
}

/// How many copies of each base object are placed in the game's cells, and how
/// many records name it in their own definitions.
///
/// The two are different questions about the same object. Count is about the
/// world: how many `REFR`s have this base, across every cell in every plugin.
/// Users is about the data: how many records hold this FormID in one of their
/// fields — a container's contents, a levelled list's entry, a script's
/// variable — which is what xEdit's "Referenced by" counts.
#[derive(Debug, Clone, Default)]
pub struct UsageCounts {
    /// Base object → how many placed references use it.
    pub count: HashMap<u32, usize>,
    /// Object → how many records name it in a field.
    pub users: HashMap<u32, usize>,
}

impl UsageCounts {
    /// How many copies of an object are placed in the world.
    pub fn copies(&self, fid: u32) -> usize {
        self.count.get(&fid).copied().unwrap_or(0)
    }

    /// How many records use an object in their own definitions.
    pub fn users(&self, fid: u32) -> usize {
        self.users.get(&fid).copied().unwrap_or(0)
    }
}

/// The usage counts of a session that has none: empty, so a column with no
/// counts yet reads as zero rather than as a missing value.
static EMPTY_USAGE: std::sync::LazyLock<UsageCounts> =
    std::sync::LazyLock::new(UsageCounts::default);

/// Whether a record signature is a placed reference, which is what the Count
/// column counts.
///
/// A cell's children hold placed references and also terrain and navmesh,
/// which are not objects a modder places; the rest are the four kinds of
/// reference the games use, including the placed hazards that carry a base
/// object of their own.
fn is_placed_sig(sig: &[u8; 4]) -> bool {
    matches!(
        sig,
        b"REFR" | b"ACRE" | b"ACHR" | b"PMRE" | b"PGRE" | b"PHZD" | b"PARW"
    )
}

/// Whether a decoded type can hold a FormID anywhere inside it.
///
/// The schema has two parallel shapes — [`nvre_esm::schema::Type`] for a
/// subrecord's own layout, and [`nvre_esm::schema::TypeDef`] for a named
/// type's — so both are walked, and a named type is followed through the
/// schema's own tables. A `formid` anywhere inside either is enough: the
/// decoded value is a tree of `0x...` strings, so one formid field means the
/// whole subrecord is worth reading.
fn type_has_formid(schema: &nvre_esm::Schema, ty: &nvre_esm::schema::Type) -> bool {
    use nvre_esm::schema::{Prim, Type};
    match ty {
        Type::Prim(p) => matches!(p, Prim::FormId),
        Type::Named(n) => schema
            .types
            .get(n)
            .or_else(|| schema.common.get(n))
            .is_some_and(|t| typedef_has_formid(schema, t)),
        Type::Inline(fields) => fields.iter().any(|f| type_has_formid(schema, &f.ty)),
        Type::Repeat(inner) => type_has_formid(schema, inner),
        Type::Array { of, .. } => type_has_formid(schema, of),
        Type::Union(variants) => variants.iter().any(|v| type_has_formid(schema, &v.ty)),
        Type::ValueUnion(vu) => vu.variants.iter().any(|v| type_has_formid(schema, &v.ty)),
        _ => false,
    }
}

/// Whether a named type can hold a FormID anywhere inside it.
///
/// A named type is a struct, an enum, a set of flags, or a union — and an
/// alias for another type, which is followed. An enum or a set of flags holds
/// integers, so only the struct's fields and the unions' variants can name a
/// record.
fn typedef_has_formid(schema: &nvre_esm::Schema, ty: &nvre_esm::schema::TypeDef) -> bool {
    let mut any = ty.fields.iter().any(|f| type_has_formid(schema, &f.ty));
    any |= ty.union.iter().any(|v| type_has_formid(schema, &v.ty));
    if let Some(alias) = &ty.alias {
        any |= type_has_formid(schema, alias);
    }
    if let Some(vu) = &ty.value_union {
        any |= vu.variants.iter().any(|v| type_has_formid(schema, &v.ty));
    }
    if let Some(pu) = &ty.peek_union {
        any |= pu.variants.iter().any(|v| type_has_formid(schema, &v.ty));
    }
    any
}

/// Every file-local FormID a subrecord's raw bytes hold, at the positions the
/// schema says.
///
/// The walk is over the schema's own layout rather than over decoded JSON: a
/// `formid` is four bytes and nothing else, so reading them directly avoids
/// the allocation and the string round-trip that make a decode cost what it
/// does. Measured against FalloutNV.esm, this is 2.9s against the decoder's
/// 17.8s for the same FormIDs.
fn scan_formids(schema: &nvre_esm::Schema, ty: &nvre_esm::schema::Type, data: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    scan(schema, ty, data, &mut pos, &mut out);
    out
}

/// Read one type from `data` at `pos`, advancing `pos` past what it read.
fn scan(
    schema: &nvre_esm::Schema,
    ty: &nvre_esm::schema::Type,
    data: &[u8],
    pos: &mut usize,
    out: &mut Vec<u32>,
) {
    use nvre_esm::schema::{Prim, Type};
    match ty {
        Type::Prim(Prim::FormId) => {
            if let Some(w) = data.get(*pos..*pos + 4) {
                let v = u32::from_le_bytes(w.try_into().unwrap());
                if v != 0 {
                    out.push(v);
                }
            }
            *pos += 4;
        }
        Type::Prim(p) => *pos += prim_size(*p),
        Type::Named(n) => {
            if let Some(t) = schema.types.get(n).or_else(|| schema.common.get(n)) {
                for f in &t.fields {
                    scan(schema, &f.ty, data, pos, out);
                }
                for v in &t.union {
                    scan(schema, &v.ty, data, pos, out);
                }
                if let Some(alias) = &t.alias {
                    scan(schema, alias, data, pos, out);
                }
            }
        }
        Type::Inline(fields) => {
            for f in fields {
                scan(schema, &f.ty, data, pos, out);
            }
        }
        Type::Repeat(inner) => {
            while *pos < data.len() {
                let before = *pos;
                scan(schema, inner, data, pos, out);
                if *pos <= before {
                    break;
                }
            }
        }
        Type::Array { of, count } => {
            let n = match count {
                nvre_esm::schema::ArrayCount::Fixed(n) => Some(*n),
                nvre_esm::schema::ArrayCount::Prefix(p) => {
                    let sz = prim_size(*p);
                    let Some(w) = data.get(*pos..*pos + sz) else {
                        return;
                    };
                    *pos += sz;
                    Some(match p {
                        Prim::U8 | Prim::I8 => u32::from(w[0]),
                        Prim::U16 | Prim::I16 => {
                            u32::from(u16::from_le_bytes(w.try_into().unwrap()))
                        }
                        _ => u32::from_le_bytes(w[..4].try_into().unwrap()),
                    } as usize)
                }
                // A count from a sibling's value: the fields are read in
                // order, so the sibling that counts is the one read before
                // this, and the walk does not track values.
                _ => None,
            };
            match n {
                Some(n) => {
                    for _ in 0..n {
                        scan(schema, of, data, pos, out);
                    }
                }
                // No count: the elements fill what is left, which for a FormID
                // is every remaining word.
                None => {
                    while *pos + 4 <= data.len() {
                        scan(schema, of, data, pos, out);
                    }
                }
            }
        }
        Type::Union(variants) => {
            // The variant whose size matches what is left, the way the decoder
            // chooses one.
            let left = data.len() - *pos;
            if let Some(v) = variants
                .iter()
                .find(|v| v.size == Some(left))
                .or_else(|| variants.last())
            {
                scan(schema, &v.ty, data, pos, out);
            }
        }
        Type::ValueUnion(vu) => {
            // The default variant: the choosing field's value is not tracked,
            // so the walk reads the default, which is the common case.
            if let Some(v) = vu.variants.get(vu.default) {
                scan(schema, &v.ty, data, pos, out);
            }
        }
        _ => {}
    }
}

/// How many bytes a primitive occupies.
///
/// A string's length is in the data, so it reads as zero here: a walk that
/// cannot read a length cannot walk past the string, and the fields after it
/// would be read from the wrong offset. A subrecord that mixes a string with a
/// FormID is read to the string and stops there, which is why the Users count
/// is a floor rather than an exact total for those records.
fn prim_size(p: nvre_esm::schema::Prim) -> usize {
    use nvre_esm::schema::Prim;
    match p {
        Prim::U8 | Prim::I8 | Prim::Bool => 1,
        Prim::U16 | Prim::I16 => 2,
        Prim::U32 | Prim::I32 | Prim::FormId | Prim::F32 | Prim::PeekU32 | Prim::Localized => 4,
        Prim::U64 | Prim::I64 | Prim::F64 => 8,
        _ => 0,
    }
}

/// Whether a row matches the text typed into the object window's filter.
///
/// The filter matches the name or the Editor ID, and the FormID as text, the
/// way GECK's Object Window does: a modder looking for one record may have any
/// of the three to hand. `filter` is already lowercased by the caller.
fn matches_text(filter: &str, fid: u32, row: &ObjectRow) -> bool {
    filter.is_empty()
        || row.label().to_lowercase().contains(filter)
        || row.edid.to_lowercase().contains(filter)
        || format!("{fid:08X}").contains(filter)
}

/// Where a top-level group sits in GECK's Object Window.
///
/// The order is the schema's, because it is a per-game fact: GECK's eight and
/// the Creation Kit's nine are not the same list. A group the schema does not
/// name sorts last, which is where a game's own additions belong rather than
/// interleaved with a heading it inherited.
pub fn group_rank(groups: &[String], group: &str) -> usize {
    groups
        .iter()
        .position(|g| g == group)
        .unwrap_or(groups.len())
}

/// Whether a record type can be placed as a reference or placed actor in a cell.
pub fn is_placeable_sig(sig: &[u8; 4]) -> bool {
    matches!(
        sig,
        b"STAT"
            | b"SCOL"
            | b"MSTT"
            | b"TREE"
            | b"FURN"
            | b"CONT"
            | b"ACTI"
            | b"LIGH"
            | b"DOOR"
            | b"TERM"
            | b"FLOR"
            | b"TACT"
            | b"PWAT"
            | b"GRAS"
            | b"WEAP"
            | b"ARMO"
            | b"AMMO"
            | b"ALCH"
            | b"BOOK"
            | b"KEYM"
            | b"MISC"
            | b"NOTE"
            | b"CHIP"
            | b"CCRD"
            | b"CMNY"
            | b"SOUN"
            | b"ASPC"
            | b"NPC_"
            | b"CREA"
    )
}

/// Collect the load order's records of the listed signatures, each with its
/// Editor ID and full name, filtered by the window's own rule.
///
/// `visible` is the object window's own list: the signatures its schema names
/// and does not hide. A record whose signature is not in it is skipped before
/// its names are read, which is what keeps the walk off the placed references
/// and cells that make up most of a master.
///
/// The filter matches the name or the Editor ID — the window's own rule, the
/// one its hint promises — and the FormID as the list shows it, so any of
/// the table's three columns can be typed into.
pub fn load_order_objects(
    lo: &LoadOrder,
    strings: &nvre_esm::strings::StringTables,
    filter: &str,
    visible: &HashSet<[u8; 4]>,
    shown: &[bool],
) -> Vec<(u32, [u8; 4], ObjectRow)> {
    let filter = filter.to_lowercase();
    let mut out: Vec<(u32, [u8; 4], ObjectRow)> = Vec::new();
    let mut seen: HashMap<u32, usize> = HashMap::new();
    for (pi, plugin) in lo.plugins.iter().enumerate() {
        // A plugin the list is not drawing from: its records are skipped
        // before they are read, which is what keeps a master the modder has
        // hidden off the walk's cost as well as off the list.
        if !shown.get(pi).copied().unwrap_or(true) {
            continue;
        }
        let plugin_strings = strings.for_plugin(&plugin.name, plugin.header.is_localized());
        for r in &lo.records[pi] {
            if !visible.contains(&r.header.sig) {
                continue;
            }
            let Some(global) = lo.to_global(pi, r.header.form_id) else {
                continue;
            };
            let (edid, full) =
                nvre_esm::inventory::names(r, plugin_strings).unwrap_or((None, None));
            let row = ObjectRow {
                fid: global,
                edid: edid.unwrap_or_default(),
                name: full.unwrap_or_default(),
            };
            if !filter.is_empty()
                && !row.label().to_lowercase().contains(&filter)
                && !format!("{global:08X}").to_lowercase().contains(&filter)
            {
                continue;
            }
            match seen.get(&global) {
                // The last definition wins, as the game reads it.
                Some(&i) => out[i] = (global, r.header.sig, row),
                None => {
                    seen.insert(global, out.len());
                    out.push((global, r.header.sig, row));
                }
            }
        }
    }
    out
}

/// Base objects a reference can be placed from, for the place palette.
///
/// `filter` is applied before the cap, so it selects from the whole load order
/// rather than only the first N alphabetically — a cap applied first would make
/// everything past `N` unfindable no matter what was typed.
pub fn placeable_base_objects(
    lo: &LoadOrder,
    strings: &nvre_esm::strings::StringTables,
    filter: &str,
    limit: usize,
) -> Vec<(u32, String, Option<String>)> {
    let filter = filter.to_lowercase();
    let mut out: Vec<(u32, String, Option<String>)> = Vec::new();
    let mut seen: HashMap<u32, usize> = HashMap::new();
    for (pi, plugin) in lo.plugins.iter().enumerate() {
        let plugin_strings = strings.for_plugin(&plugin.name, plugin.header.is_localized());
        for r in &lo.records[pi] {
            if !is_placeable_sig(&r.header.sig) {
                continue;
            }
            let Some(global) = lo.to_global(pi, r.header.form_id) else {
                continue;
            };
            let (edid, full) =
                nvre_esm::inventory::names(r, plugin_strings).unwrap_or((None, None));
            let label = full.or(edid).unwrap_or_else(|| format!("{global:08X}"));
            if !filter.is_empty() && !label.to_lowercase().contains(&filter) {
                continue;
            }
            match seen.get(&global) {
                // The last definition wins, as the game reads it. The slot is
                // looked up by the FormID it was filed under, not the record's
                // position, which changes once the list is sorted.
                Some(&i) => out[i] = (global, label, None),
                None => {
                    seen.insert(global, out.len());
                    out.push((global, label, None));
                }
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    out.truncate(limit);
    out
}

/// Record types that can be placed as a reference.
pub const PLACEABLE: [&[u8; 4]; 27] = [
    b"STAT", b"SCOL", b"MSTT", b"TREE", b"FURN", b"CONT", b"ACTI", b"LIGH", b"DOOR", b"TERM",
    b"FLOR", b"TACT", b"PWAT", b"GRAS", b"WEAP", b"ARMO", b"AMMO", b"ALCH", b"BOOK", b"KEYM",
    b"MISC", b"NOTE", b"CHIP", b"CCRD", b"CMNY", b"SOUN", b"ASPC",
];

/// A selected record, either already in the plugin or staged for the save.
#[derive(Debug, Clone, Copy)]
pub enum StagedRecord<'a> {
    /// In the plugin being edited.
    Saved(&'a nvre_esm::Record),
    /// Queued for insertion, with a placeholder FormID.
    Pending(&'a nvre_esm::Record),
}

impl StagedRecord<'_> {
    /// The record itself, either way.
    pub fn record(&self) -> &nvre_esm::Record {
        match self {
            StagedRecord::Saved(r) | StagedRecord::Pending(r) => r,
        }
    }
}
