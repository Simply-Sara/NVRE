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
use std::sync::{Arc, RwLock};

type CachedRecordList = Arc<[(u32, String)]>;

/// The open editing session.
#[derive(Resource)]
pub struct Session {
    /// The plugin being edited.
    pub doc: Document,
    /// The load order the plugin was opened in (masters first).
    pub plugins: &'static [fnv_esm::Plugin],
    /// The load order index, built once at startup.
    pub lo: LoadOrder<'static>,
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
    /// Cached exterior grid references, and (cell, radius) they were built for.
    exterior_refs: Vec<RefRow>,
    exterior_refs_key: Option<(u32, i32)>,
    /// Base FormID → model path, resolved once per base object.
    models: HashMap<u32, Option<String>>,
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
    /// Records the session has queued for deletion, in the order asked.
    deleted: Vec<u32>,
    /// Status line for the UI.
    pub status: String,
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
        let leaked_plugins: &'static [fnv_esm::Plugin] = Box::leak(plugins.into_boxed_slice());
        let lo = LoadOrder::new(leaked_plugins).map_err(|e| e.to_string())?;
        let index = fnv_world::WorldIndex::build(&lo, &schema);

        let mut persistent_by_grid: HashMap<(u32, (i32, i32)), Vec<u32>> = HashMap::new();
        for (&w_fid, ws) in &index.worldspaces {
            if let Some(pc_id) = ws.persistent_cell {
                if let Some(pc) = index.cells.get(&pc_id) {
                    for &r in &pc.refs {
                        if let Some(p) = fnv_world::placed::placement(&lo, &schema, r) {
                            let g = fnv_world::cell_of(p.position[0], p.position[1]);
                            persistent_by_grid.entry((w_fid, g)).or_default().push(r);
                        }
                    }
                }
            }
        }

        Ok(Session {
            doc,
            plugins: leaked_plugins,
            lo,
            schema,
            index,
            masters,
            cell: None,
            selected: None,
            placed_index: 0,
            refs: Vec::new(),
            refs_cell: None,
            exterior_refs: Vec::new(),
            exterior_refs_key: None,
            models: HashMap::new(),
            details: RefDetails::default(),
            details_cell: None,
            palette_groups: Vec::new(),
            palette_filter: String::new(),
            deleted: Vec::new(),
            status: String::new(),
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
    pub fn create(
        name: &str,
        is_master: bool,
        data_dir: &std::path::Path,
        schema_dir: &std::path::Path,
        masters: Vec<String>,
    ) -> Result<Session, String> {
        let schema = Schema::load_dir(schema_dir).map_err(|e| e.to_string())?;
        let mut plugins: Vec<fnv_esm::Plugin> = Vec::new();
        for m in &masters {
            let p = data_dir.join(m);
            plugins.push(fnv_esm::Plugin::open(&p).map_err(|e| format!("{m}: {e}"))?);
        }
        let leaked_plugins: &'static [fnv_esm::Plugin] = Box::leak(plugins.into_boxed_slice());
        let lo = LoadOrder::new(leaked_plugins).map_err(|e| e.to_string())?;
        let index = fnv_world::WorldIndex::build(&lo, &schema);

        let mut persistent_by_grid: HashMap<(u32, (i32, i32)), Vec<u32>> = HashMap::new();
        for (&w_fid, ws) in &index.worldspaces {
            if let Some(pc_id) = ws.persistent_cell {
                if let Some(pc) = index.cells.get(&pc_id) {
                    for &r in &pc.refs {
                        if let Some(p) = fnv_world::placed::placement(&lo, &schema, r) {
                            let g = fnv_world::cell_of(p.position[0], p.position[1]);
                            persistent_by_grid.entry((w_fid, g)).or_default().push(r);
                        }
                    }
                }
            }
        }

        let doc = Document::create_empty(name, is_master, &masters).map_err(|e| e.to_string())?;

        Ok(Session {
            doc,
            plugins: leaked_plugins,
            lo,
            schema,
            index,
            masters,
            cell: None,
            selected: None,
            placed_index: 0,
            refs: Vec::new(),
            refs_cell: None,
            exterior_refs: Vec::new(),
            exterior_refs_key: None,
            models: HashMap::new(),
            details: RefDetails::default(),
            details_cell: None,
            palette_groups: Vec::new(),
            palette_filter: String::new(),
            deleted: Vec::new(),
            status: format!("created new plugin {name}"),
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
    pub fn plugin(&self) -> &fnv_esm::Plugin {
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
                            fnv_world::placed::base_object(lo, &self.schema, b)
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
        let b = fnv_world::placed::base_object(lo, &self.schema, base)?;
        b.world_model_path().or_else(|| b.model_path())
    }

    /// The object palette, grouped into GECK's categories.
    ///
    /// GECK's Object Window is a category list on the left and that category's
    /// objects on the right, so the palette is built the same way: one group per
    /// placeable record type, named the way GECK names it ("Static",
    /// "Container"), each holding its objects sorted by name.
    ///
    /// The query walks the load order, so it is worth hundreds of milliseconds;
    /// the panel asks for it every frame otherwise, and the result is cached
    /// until the filter changes. The filter is applied before the cap, so it
    /// selects from the whole load order rather than only the first N
    /// alphabetically.
    pub fn object_groups(&mut self, filter: &str) -> &[ObjectGroup] {
        if self.palette_filter != filter || self.palette_groups.is_empty() {
            let found = (|| {
                let lo = self.load_order().ok()?;
                let objects = load_order_objects(lo, filter);
                // Grouped by the record type, which is the category. One
                // load-order lookup per distinct type rather than per object.
                let mut types: HashMap<[u8; 4], (String, String)> = HashMap::new();
                for (_, sig, _) in &objects {
                    types.entry(*sig).or_insert_with(|| {
                        let group = geck_group_for_sig(sig)
                            .unwrap_or("Miscellaneous")
                            .to_string();
                        let category = self
                            .schema
                            .records
                            .get(sig)
                            .map(|d| d.name.clone())
                            .unwrap_or_else(|| String::from_utf8_lossy(sig).to_string());
                        (group, category)
                    });
                }
                let mut groups: Vec<ObjectGroup> = Vec::new();
                for (fid, sig, label) in objects {
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
                    groups[at].objects.push((fid, label));
                }
                // Categories ordered by GECK top-level group, then alphabetically by category.
                groups.sort_by(|a, b| {
                    group_rank(&a.group)
                        .cmp(&group_rank(&b.group))
                        .then_with(|| a.category.cmp(&b.category))
                });
                for g in &mut groups {
                    g.objects.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
                }
                Some(groups)
            })()
            .unwrap_or_default();
            self.palette_groups = found;
            self.palette_filter = filter.to_string();
        }
        &self.palette_groups
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
        for (pi, _plugin) in self.lo.plugins.iter().enumerate() {
            for r in &self.lo.records[pi] {
                if r.header.sig == sig {
                    if let Some(global) = self.lo.to_global(pi, r.header.form_id) {
                        if seen.insert(global) {
                            let (edid, full) = fnv_esm::inventory::names(r).unwrap_or((None, None));
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
                    fnv_world::fields::winner(lo, owner)
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
                if let Some((_, rec)) = fnv_world::fields::winner(lo, base) {
                    types.entry(base).or_insert_with(|| {
                        self.schema
                            .records
                            .get(&rec.header.sig)
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
                    let (_, r) = fnv_world::fields::winner(lo, fid)?;
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

    /// Select a reference.
    pub fn select(&mut self, form_id: u32) {
        self.selected = Some(form_id);
    }

    /// Open any object by FormID and signature in its specialized editor window.
    pub fn open_object(&mut self, form_id: u32, sig: [u8; 4]) {
        self.editing_object = Some((form_id, sig));
        self.status = format!(
            "opened {} {:08X} in editor window",
            fnv_util::tag_str(&sig),
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
        let (_, r) = fnv_world::fields::winner(lo, fid)?;
        Some(StagedRecord::Saved(r))
    }

    /// The signature of a record by FormID.
    pub fn record_sig(&self, form_id: u32) -> Option<[u8; 4]> {
        if let Some(r) = self.doc.record(form_id) {
            return Some(r.header.sig);
        }
        let lo = self.load_order().ok()?;
        let (_, r) = fnv_world::fields::winner(lo, form_id)?;
        Some(r.header.sig)
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
            return Err(format!("{fid:08X} has no {}", fnv_util::tag_str(sig)));
        };
        let mut decoded = nvre_edit::encode::decoded(&self.schema, &rec_sig, sub)
            .ok_or_else(|| format!("{} does not decode", fnv_util::tag_str(sig)))?;
        set_path(&mut decoded, path, value)?;
        let bytes = nvre_edit::encode::encode_sub(&self.schema, &rec_sig, sig, &decoded)
            .map_err(|e| e.to_string())?;
        // A staged record has FormID 0 until the save; the edit has to reach the
        // staged copy, which is addressed by that same placeholder.
        self.doc.set_sub(fid, sig, bytes);
        self.status = format!("set {}.{} on {fid:08X}", fnv_util::tag_str(sig), path);
        Ok(())
    }

    /// Queue the selected reference for deletion.
    pub fn delete_selected(&mut self) {
        if let Some(fid) = self.selected.take() {
            if !self.deleted.contains(&fid) {
                self.deleted.push(fid);
            }
            self.doc.delete_record(fid);
            self.status = format!("queued {fid:08X} for deletion");
            self.refs_cell = None;
            self.exterior_refs_key = None;
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

    /// Where a save goes: beside the plugin being edited, as a `.esp`.
    pub fn save_target(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(self.doc.name())
    }

    /// Save as a new plugin.
    pub fn save(&self, out: &std::path::Path) -> Result<nvre_edit::SaveReport, String> {
        let masters = nvre_edit::MasterList::new(self.masters.clone());
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
pub fn placement_of(rec: &fnv_esm::Record) -> Option<([f32; 3], [f32; 3])> {
    placement_in(&rec.subrecords().ok()?)
}

/// [`placement_of`] over subrecords that are already parsed.
fn placement_in(subs: &[fnv_esm::Subrecord]) -> Option<([f32; 3], [f32; 3])> {
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

/// A FormID-valued subrecord, as the file stores it.
///
/// The value is file-local: mapping it to the load order needs the plugin's
/// master list, so a caller that shows the FormID resolves it once per owner
/// rather than once per reference (see [`Session::ref_details`]).
///
/// 0 is the null reference and no owner at all, so it reads as `None` rather
/// than as the master's own range.
fn formid_in(subs: &[fnv_esm::Subrecord], sig: &[u8; 4]) -> Option<u32> {
    let s = subs.iter().find(|s| &s.sig == sig)?;
    let local = <[u8; 4]>::try_from(s.data.get(..4)?)
        .ok()
        .map(u32::from_le_bytes)?;
    (local != 0).then_some(local)
}

/// A reference's scale, from its XSCL subrecord.
///
/// Defaults to 1.0 when XSCL is absent. When present, XSCL is a 4-byte little-endian float.
fn scale_in(subs: &[fnv_esm::Subrecord]) -> Option<f32> {
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
fn lock_in(subs: &[fnv_esm::Subrecord]) -> Option<LockRow> {
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
    /// The category's objects, by base FormID and name, sorted by name.
    pub objects: Vec<(u32, String)>,
}

/// Where a top-level group sits in GECK's Object Window.
pub fn group_rank(group: &str) -> usize {
    const ORDER: [&str; 8] = [
        "Actors",
        "Actor Data",
        "Items",
        "Magic",
        "Special Effects",
        "WorldObjects",
        "Audio",
        "Miscellaneous",
    ];
    ORDER
        .iter()
        .position(|g| *g == group)
        .unwrap_or(ORDER.len())
}

/// The top-level GECK group for a record signature.
pub fn geck_group_for_sig(sig: &[u8; 4]) -> Option<&'static str> {
    match sig {
        b"CREA" | b"LVLC" | b"NPC_" | b"LVLN" => Some("Actors"),
        b"BPTD" | b"CLAS" | b"CSTY" | b"EYES" | b"FACT" | b"HAIR" | b"HDPT" | b"PACK" | b"PERK"
        | b"QUST" | b"RACE" | b"RGDL" | b"VTYP" | b"AVIF" => Some("Actor Data"),
        b"AMMO" | b"AMEF" | b"ARMO" | b"ARMA" | b"BOOK" | b"CCRD" | b"CMNY" | b"CHIP" | b"CDCK"
        | b"INGR" | b"KEYM" | b"LVLI" | b"MISC" | b"NOTE" | b"WEAP" | b"IMOD" => Some("Items"),
        b"ALCH" | b"MGEF" | b"ENCH" | b"SPEL" => Some("Magic"),
        b"ADDN" | b"DEBR" | b"EFSH" | b"EXPL" | b"IMGS" | b"IMAD" | b"IPDS" | b"IPCT" | b"PROJ" => {
            Some("Special Effects")
        }
        b"ACTI" | b"CLMT" | b"CONT" | b"DOOR" | b"FURN" | b"GRAS" | b"LIGH" | b"MSTT" | b"PWAT"
        | b"REGN" | b"STAT" | b"SCOL" | b"TACT" | b"TERM" | b"TREE" | b"WATR" | b"WTHR" => {
            Some("WorldObjects")
        }
        b"ASPC" | b"ALOC" | b"MSET" | b"MUSC" | b"SOUN" => Some("Audio"),
        _ => Some("Miscellaneous"),
    }
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

/// Record types that are not individual objects in GECK's Object Window
/// (cells, worldspaces, landscapes, navmeshes, dialogue responses, and placed instances).
pub const NON_OBJECT_SIGS: [&[u8; 4]; 12] = [
    b"REFR", b"ACHR", b"ACRE", b"PGRE", b"CELL", b"LAND", b"NAVM", b"NAVI", b"WRLD", b"LTEX",
    b"INFO", b"DIAL",
];

/// Collect all objects across the load order, filtered by label.
pub fn load_order_objects(lo: &LoadOrder, filter: &str) -> Vec<(u32, [u8; 4], String)> {
    let filter = filter.to_lowercase();
    let mut out: Vec<(u32, [u8; 4], String)> = Vec::new();
    let mut seen: HashMap<u32, usize> = HashMap::new();
    for (pi, _plugin) in lo.plugins.iter().enumerate() {
        for r in &lo.records[pi] {
            if NON_OBJECT_SIGS.contains(&&r.header.sig) {
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
                Some(&i) => out[i] = (global, r.header.sig, label),
                None => {
                    seen.insert(global, out.len());
                    out.push((global, r.header.sig, label));
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
    filter: &str,
    limit: usize,
) -> Vec<(u32, String, Option<String>)> {
    let filter = filter.to_lowercase();
    let mut out: Vec<(u32, String, Option<String>)> = Vec::new();
    let mut seen: HashMap<u32, usize> = HashMap::new();
    for (pi, _plugin) in lo.plugins.iter().enumerate() {
        for r in &lo.records[pi] {
            if !is_placeable_sig(&r.header.sig) {
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
pub const PLACEABLE: [&[u8; 4]; 27] = [
    b"STAT", b"SCOL", b"MSTT", b"TREE", b"FURN", b"CONT", b"ACTI", b"LIGH", b"DOOR", b"TERM",
    b"FLOR", b"TACT", b"PWAT", b"GRAS", b"WEAP", b"ARMO", b"AMMO", b"ALCH", b"BOOK", b"KEYM",
    b"MISC", b"NOTE", b"CHIP", b"CCRD", b"CMNY", b"SOUN", b"ASPC",
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
