//! The editor's session state: what plugin is open, which cell is being edited,
//! and which reference is selected.
//!
//! This is deliberately plain data with no Bevy in it, so the rules about what
//! is selected and what has changed can be tested without a window. The
//! viewport reads it to decide what to draw; the panels read and write it.

use bevy::prelude::*;
use fnv_esm::{LoadOrder, Schema};
use nvre_edit::Document;
use std::collections::HashMap;

/// The open editing session.
#[derive(Resource)]
pub struct Session {
    /// The plugin being edited.
    pub doc: Document,
    /// The load order the plugin was opened in (masters first).
    pub plugins: Vec<fnv_esm::Plugin>,
    /// The ESM record schema.
    pub schema: Schema,
    /// The world index: which reference is in which cell.
    pub index: fnv_world::WorldIndex,
    /// The masters the saved plugin will depend on.
    pub masters: Vec<String>,
    /// The cell currently open, by FormID.
    pub cell: Option<u32>,
    /// The selected reference, by FormID (0 for a record not yet saved).
    pub selected: Option<u32>,
    /// How many records this session has placed, for resolving a placeholder.
    placed_index: usize,
    /// The cached reference list, and the cell it was built for.
    refs: Vec<RefRow>,
    refs_cell: Option<u32>,
    /// Base FormID → model path, resolved once per base object.
    models: HashMap<u32, Option<String>>,
    /// The place palette's query result, and the filter it was built for.
    ///
    /// Building it walks the load order, which is the editor's most expensive
    /// query; doing it every frame is what made the editor run at half a frame
    /// per second while a cell was open.
    palette: Vec<(u32, String)>,
    palette_filter: String,
    /// Records the session has queued for deletion, in the order asked.
    deleted: Vec<u32>,
    /// Status line for the UI.
    pub status: String,
}

impl Session {
    /// Open a plugin for editing, with the masters it depends on already loaded.
    pub fn open(
        path: &std::path::Path,
        schema_dir: &std::path::Path,
        masters: Vec<String>,
    ) -> Result<Session, String> {
        let schema = Schema::load_dir(schema_dir).map_err(|e| e.to_string())?;
        // Masters are named as file names and resolved beside the plugin being
        // edited, so `--masters FalloutNV.esm` works whatever the working folder.
        let dir = path.parent().unwrap_or(std::path::Path::new("."));
        let mut plugins: Vec<fnv_esm::Plugin> = Vec::new();
        for m in &masters {
            let p = dir.join(m);
            plugins.push(fnv_esm::Plugin::open(&p).map_err(|e| format!("{m}: {e}"))?);
        }
        let doc = Document::open(path).map_err(|e| e.to_string())?;
        let own = doc.name().to_string();
        // The plugin being edited goes after its masters, unless it is one of
        // them (editing a master directly), in which case it is already loaded.
        if !plugins.iter().any(|p| p.name.eq_ignore_ascii_case(&own)) {
            plugins.push(fnv_esm::Plugin::open(path).map_err(|e| format!("{own}: {e}"))?);
        }
        let lo = LoadOrder::new(&plugins).map_err(|e| e.to_string())?;
        let index = fnv_world::WorldIndex::build(&lo, &schema);

        Ok(Session {
            doc,
            plugins,
            schema,
            index,
            masters,
            cell: None,
            selected: None,
            placed_index: 0,
            refs: Vec::new(),
            refs_cell: None,
            models: HashMap::new(),
            palette: Vec::new(),
            palette_filter: String::new(),
            deleted: Vec::new(),
            status: String::new(),
        })
    }

    /// The plugin being edited.
    pub fn plugin(&self) -> &fnv_esm::Plugin {
        self.doc.plugin()
    }

    /// The load order, rebuilt on demand (cheap enough for a UI's frame budget).
    pub fn load_order(&self) -> Result<LoadOrder<'_>, String> {
        LoadOrder::new(&self.plugins).map_err(|e| e.to_string())
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
                            fnv_world::placed::base_object(&lo, &self.schema, b)
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

    /// One base object's model path, with no caching.
    fn resolve_model_path(&mut self, base: u32) -> Option<String> {
        let lo = self.load_order().ok()?;
        let b = fnv_world::placed::base_object(&lo, &self.schema, base)?;
        b.world_model_path().or_else(|| b.model_path())
    }

    /// The place palette for a filter, cached until the filter changes.
    ///
    /// The query walks the load order, so it is worth hundreds of milliseconds;
    /// the panel asks for it every frame otherwise.
    pub fn palette(&mut self, filter: &str) -> Vec<(u32, String)> {
        if self.palette_filter == filter && !self.palette.is_empty() {
            return self.palette.clone();
        }
        let found = (|| {
            let lo = self.load_order().ok()?;
            Some(
                placeable_base_objects(&lo, filter, 200)
                    .into_iter()
                    .map(|(f, l, _)| (f, l))
                    .collect::<Vec<_>>(),
            )
        })()
        .unwrap_or_default();
        self.palette = found;
        self.palette_filter = filter.to_string();
        self.palette.clone()
    }

    /// Every cell in the load order, as (FormID, editor ID, interior?).
    pub fn cells(&self) -> Vec<CellRow> {
        let mut out: Vec<CellRow> = self
            .index
            .cells
            .values()
            .map(|c| CellRow {
                form_id: c.form_id,
                edid: c.edid.clone(),
                interior: c.is_interior(),
                refs: c.refs.len(),
                world: c.world.is_some(),
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

    fn build_cell_refs(&self, cell: u32) -> Vec<RefRow> {
        let Some(c) = self.index.cells.get(&cell) else {
            return Vec::new();
        };
        c.refs
            .iter()
            .filter_map(|&fid| {
                let e = self.index.refs.get(&fid)?;
                let rec = self.doc.record(fid)?;
                // Read once, for both the position and the rotation.
                let placement = placement_of(rec);
                Some(RefRow {
                    form_id: fid,
                    sig: String::from_utf8_lossy(&e.sig).to_string(),
                    edid: nvre_edit::document::edid_of(rec),
                    base: nvre_edit::document::value_of(&self.schema, rec, b"NAME")
                        .and_then(|v| v.as_str().map(str::to_string)),
                    persistent: e.persistent,
                    position: placement.map(|(p, _)| p),
                    rotation: placement.map(|(_, r)| r),
                })
            })
            .collect()
    }

    /// Open a cell for editing.
    pub fn open_cell(&mut self, cell: u32) {
        self.refs.clear();
        self.refs_cell = None;
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

    /// Select a reference.
    pub fn select(&mut self, form_id: u32) {
        self.selected = Some(form_id);
    }

    /// The selected reference's record.
    ///
    /// A newly placed record is staged with FormID 0 and lives in the session's
    /// pending inserts, not in the plugin, so it is read from there.
    pub fn selected_record(&self) -> Option<StagedRecord<'_>> {
        let fid = self.selected?;
        if fid == 0 {
            let p = self.doc.inserted().last()?;
            return Some(StagedRecord::Pending(&p.record));
        }
        self.doc.record(fid).map(StagedRecord::Saved)
    }

    /// Queue a subrecord edit on the selected reference.
    pub fn edit_selected(&mut self, sig: &[u8; 4], data: Vec<u8>) {
        if let Some(fid) = self.selected {
            // A staged record has FormID 0 until the save; the edit has to reach
            // the staged copy, which is addressed by that same placeholder.
            self.doc.set_sub(fid, sig, data);
            self.status = format!("queued an edit on {fid:08X}");
        }
    }

    /// Queue the selected reference for deletion.
    pub fn delete_selected(&mut self) {
        if let Some(fid) = self.selected.take() {
            if !self.deleted.contains(&fid) {
                self.deleted.push(fid);
            }
            self.doc.delete_record(fid);
            self.status = format!("queued {fid:08X} for deletion");
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
        let rec = if persistent {
            nvre_edit::document::make_refr_flags(
                next,
                base,
                position,
                [0.0, 0.0, 0.0],
                None,
                None,
                nvre_edit::document::REFR_PERSISTENT,
            )
        } else {
            nvre_edit::document::make_refr(next, base, position, [0.0, 0.0, 0.0], None, None)
        };
        self.doc
            .insert(nvre_edit::document::PendingRecord { cell, record: rec });
        self.selected = Some(next);
        self.status = format!("placed {base:08X} at {position:?}");
    }

    /// Move the selected reference to a game position.
    pub fn move_selected(&mut self, position: [f32; 3]) {
        let _fid = self.selected;
        let Some(rec) = self.selected_record() else {
            return;
        };
        let rec = rec.record();
        let Some(current) = nvre_edit::document::value_of(&self.schema, rec, b"DATA") else {
            return;
        };
        let mut next = current;
        if let Some(p) = next.get_mut("position").and_then(|p| p.as_object_mut()) {
            p.insert("x".into(), serde_json::json!(position[0] as f64));
            p.insert("y".into(), serde_json::json!(position[1] as f64));
            p.insert("z".into(), serde_json::json!(position[2] as f64));
        }
        match nvre_edit::encode::encode_sub(&self.schema, b"REFR", b"DATA", &next) {
            Ok(bytes) => self.edit_selected(b"DATA", bytes),
            Err(e) => self.status = format!("cannot encode the move: {e}"),
        }
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

    /// Where a save goes: beside the plugin being edited, as a `.esp`.
    pub fn save_target(&self) -> std::path::PathBuf {
        let name = self.doc.name();
        let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
        std::path::Path::new(stem)
            .with_extension("esp")
            .to_path_buf()
    }

    /// Save as a new plugin.
    pub fn save(&self, out: &std::path::Path) -> Result<nvre_edit::SaveReport, String> {
        let masters = nvre_edit::MasterList::new(self.masters.clone());
        nvre_edit::write_plugin(&self.doc, &masters, &self.schema, out)
    }
}

/// A cell, for the browser list.
#[derive(Debug, Clone)]
pub struct CellRow {
    pub form_id: u32,
    pub edid: Option<String>,
    pub interior: bool,
    pub refs: usize,
    pub world: bool,
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
}

/// A reference's position and rotation from its DATA subrecord.
///
/// DATA is 24 bytes: position then rotation, both xyz f32, in game units and
/// radians. The rotation is what makes a door swing the right way and a road run
/// the right direction, so it is read alongside the position rather than left
/// for the caller to find.
pub fn placement_of(rec: &fnv_esm::Record) -> Option<([f32; 3], [f32; 3])> {
    let subs = rec.subrecords().ok()?;
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
pub fn position_of(rec: &fnv_esm::Record) -> Option<[f32; 3]> {
    placement_of(rec).map(|(p, _)| p)
}

/// Base objects that can be placed, for the place-new palette.
///
/// Read from the load order so the palette lists what the game actually ships
/// rather than a hardcoded set.
/// Base objects a reference can be placed from.
///
/// `filter` is applied before the cap, so it selects from the whole load order
/// rather than only the first N alphabetically — a cap applied first would make
/// everything past `N` unfindable no matter what was typed.
pub fn placeable_base_objects(
    lo: &LoadOrder,
    filter: &str,
    limit: usize,
) -> Vec<(u32, String, Option<String>)> {
    let filter = filter.to_lowercase();
    let mut out: Vec<(u32, String, Option<String>)> = Vec::new();
    let mut seen: HashMap<u32, usize> = HashMap::new();
    for (pi, _plugin) in lo.plugins.iter().enumerate() {
        for r in &lo.records[pi] {
            if !PLACEABLE.contains(&&r.header.sig) {
                continue;
            }
            let Some(global) = lo.to_global(pi, r.header.form_id) else {
                continue;
            };
            let (edid, full) = fnv_esm::inventory::names(r).unwrap_or((None, None));
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
const PLACEABLE: [&[u8; 4]; 8] = [
    b"STAT", b"SCOL", b"MSTT", b"TREE", b"FURN", b"CONT", b"ACTI", b"LIGH",
];

/// A selected record, either already in the plugin or staged for the save.
#[derive(Debug, Clone, Copy)]
pub enum StagedRecord<'a> {
    /// In the plugin being edited.
    Saved(&'a fnv_esm::Record),
    /// Queued for insertion, with a placeholder FormID.
    Pending(&'a fnv_esm::Record),
}

impl StagedRecord<'_> {
    /// The record itself, either way.
    pub fn record(&self) -> &fnv_esm::Record {
        match self {
            StagedRecord::Saved(r) | StagedRecord::Pending(r) => r,
        }
    }
}
