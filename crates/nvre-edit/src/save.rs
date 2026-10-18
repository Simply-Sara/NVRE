//! Saving a document as a **new plugin**, which is what makes it a mod.
//!
//! `Document::save` copies the plugin it opened, so the output carries the
//! master's own FormIDs and its master list. The game cannot load that as a
//! plugin: a FormID is file-local, its top byte indexing *that file's* master
//! list, so the copied references point at nothing.
//!
//! Saving as a plugin therefore has to do three things `save` does not:
//!
//! 1. **Write a TES4 header** naming the masters this plugin depends on
//!    (`MAST`/`DATA` pairs), with the ESM flag off so the file is a `.esp`.
//! 2. **Remap every FormID** into the new file's numbering, through the load
//!    order, so a reference to `FalloutNV.esm`'s object is written in the new
//!    plugin's master-list index.
//! 3. **Allocate FormIDs** for new records from `next_object_id`, so two inserts
//!    in one session cannot collide.
//!
//! The record structure is copied unchanged: cells, groups and untouched records
//! keep their original bytes, and only FormID-bearing subrecords are rewritten.
//! A record whose FormIDs cannot be remapped safely is reported, not silently
//! mangled.

use fnv_esm::raw::{record_flags, Node, Record, Subrecord};
use fnv_esm::schema::Schema;
use fnv_esm::PluginHeader;
use std::collections::HashMap;

/// What a save produced, for reporting to the user.
#[derive(Debug, Clone, Default)]
pub struct SaveReport {
    /// Records written.
    pub records: usize,
    /// Records newly inserted by this session.
    pub inserted: usize,
    /// Records marked deleted by this session.
    pub deleted: usize,
    /// Subrecords whose FormIDs were rewritten.
    pub remapped_subs: usize,
    /// FormIDs rewritten inside those subrecords.
    pub remapped_formids: usize,
    /// Subrecords that could not be remapped and were left as-is.
    pub unremapped: Vec<String>,
    /// Bytes written.
    pub bytes: usize,
}

/// The masters the new plugin depends on, in load order.
///
/// The first entry must be the plugin's own primary master (for FNV that is
/// `FalloutNV.esm`); the rest follow in load order. Names are file names,
/// e.g. `["FalloutNV.esm", "DeadMoney.esm"]`.
#[derive(Debug, Clone)]
pub struct MasterList {
    pub names: Vec<String>,
    /// Author and description for the header's CNAM/SNAM.
    pub author: Option<String>,
    pub description: Option<String>,
}

impl MasterList {
    /// The load order the editor was opened with.
    pub fn new(names: Vec<String>) -> MasterList {
        MasterList {
            names,
            author: None,
            description: None,
        }
    }
}

/// The FormID map for writing `doc`'s records into a plugin whose master list
/// is `masters`.
///
/// The source's own records (top byte == its master count) become records of the
/// new file; references to the source's masters become references to the same
/// masters by their index in the new list.
///
/// Returns `Err` when a master the source needs is missing from the new list,
/// which would leave dangling references.
pub fn formid_map_for(
    doc: &crate::Document,
    masters: &MasterList,
) -> Result<HashMap<u32, u32>, String> {
    let new_self = masters.names.len() as u32;
    formid_map_with(doc, masters, new_self)
}

/// [`formid_map_for`] with an explicit target for the source's own records.
///
/// `self_target` is the load-order index the source's own FormIDs should point
/// at: the new plugin's own index for records the mod defines itself, or the
/// source's position in the master list for records that *override* it.
fn formid_map_with(
    doc: &crate::Document,
    masters: &MasterList,
    self_target: u32,
) -> Result<HashMap<u32, u32>, String> {
    let plugin = doc.plugin();
    let src_self = plugin.self_index() as u32;
    let mut map = HashMap::new();

    for (src_master_load, m) in plugin.header.masters.iter().enumerate() {
        let src_master_load = src_master_load as u32;
        let new_master_load = masters
            .names
            .iter()
            .position(|n| n.eq_ignore_ascii_case(&m.name))
            .ok_or_else(|| format!("master {} is not in the new plugin's master list", m.name))?
            as u32;
        map.insert(src_master_load, new_master_load);
    }
    map.insert(src_self, self_target);
    Ok(map)
}

/// The index of the plugin being edited in the new plugin's master list.
///
/// A mod that overrides another plugin's records must depend on it, so this is
/// an error when the source is absent from the master list.
fn source_master_index(doc: &crate::Document, masters: &MasterList) -> Result<u32, String> {
    let name = doc.plugin().name.clone();
    masters
        .names
        .iter()
        .position(|n| n.eq_ignore_ascii_case(&name))
        .map(|i| i as u32)
        .ok_or_else(|| {
            format!(
                "the plugin being edited ({name}) must be in the new plugin's master list to override its records"
            )
        })
}

/// What the saved plugin should contain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Only this session's new records and edits. The output is a small mod that
    /// overrides nothing it did not touch — what a mod should be.
    OwnRecords,
    /// Every record of the source plugin, remapped. Useful for round-tripping a
    /// plugin through the editor, but the result reads as overriding everything.
    WholePlugin,
}

/// Write `doc`'s records as a new plugin.
///
/// `schema` is used to find and rewrite FormID-bearing subrecords. With
/// [`Scope::OwnRecords`] only the session's own records are written, so the
/// output overrides nothing it did not touch and stays small; with
/// [`Scope::WholePlugin`] every record of the source is remapped and written,
/// which is what a round-trip through the editor needs.
pub fn write_plugin(
    doc: &crate::Document,
    masters: &MasterList,
    schema: &Schema,
    out_path: &std::path::Path,
) -> Result<SaveReport, String> {
    write_plugin_with_load_order(doc, masters, schema, None, out_path, Scope::OwnRecords)
}

/// [`write_plugin`] with an explicit [`Scope`].
pub fn write_plugin_scoped(
    doc: &crate::Document,
    masters: &MasterList,
    schema: &Schema,
    out_path: &std::path::Path,
    scope: Scope,
) -> Result<SaveReport, String> {
    write_plugin_with_load_order(doc, masters, schema, None, out_path, scope)
}

/// Write `doc`'s records with an optional load order to resolve overrides from masters.
pub fn write_plugin_with_load_order(
    doc: &crate::Document,
    masters: &MasterList,
    schema: &Schema,
    lo: Option<&fnv_esm::LoadOrder<'_>>,
    out_path: &std::path::Path,
    scope: Scope,
) -> Result<SaveReport, String> {
    let map = formid_map_for(doc, masters)?;
    let new_self = masters.names.len() as u32;

    let mut report = SaveReport::default();
    let mut problems: Vec<String> = Vec::new();

    // Start from the source's structure so cells and groups keep their shape,
    // then decide which records survive.
    let mut nodes = doc.plugin().nodes.clone();
    rewrite_header(&mut nodes, masters, new_self)?;

    let is_master_override = masters
        .names
        .iter()
        .any(|n| n.eq_ignore_ascii_case(&doc.plugin().name));

    if scope == Scope::WholePlugin {
        let mut walk = |node: &mut Node| {
            if let Node::Record(r) = node {
                report.records += 1;
                remap_record(r, schema, &map, &mut report, &mut problems);
            }
        };
        for n in &mut nodes {
            walk_node(n, &mut walk);
        }
    } else if is_master_override {
        // A mod carries its own records: drop the source's records and keep only
        // the group structure the new ones hang from.
        prune_records(&mut nodes);
        report.records = 0;

        // Editing or deleting a record that belongs to another plugin means
        // *overriding* it: the mod must carry a copy of that record with the
        // change applied, keeping the original FormID so the load order knows
        // which record is being overridden. Overrides are declared in the
        // header's ONAM array.
        let src_index = source_master_index(doc, masters)?;
        let override_map = formid_map_with(doc, masters, src_index)?;
        let mut overrides: Vec<u32> = Vec::new();

        // Every FormID the session touched: edits and deletes alike.
        let mut touched: Vec<u32> = doc.deleted().to_vec();
        touched.extend(doc.edits().map(|(f, _)| *f));
        touched.sort_unstable();
        touched.dedup();

        for fid in touched {
            // Only records the source actually defines can be overridden.
            let Some(orig) = find_record(doc.nodes(), fid) else {
                problems.push(format!(
                    "record {fid:08X} is not in {}; the mod cannot change it",
                    doc.plugin().name
                ));
                continue;
            };
            let mut rec = orig.clone();
            // The override keeps the record's original FormID, whose top byte
            // is the overridden plugin's index in the new master list.
            rec.header.form_id = (src_index << 24) | (fid & 0x00FF_FFFF);
            if let Some(edits) = doc.edits_for(fid) {
                crate::document::apply_edits_pub(&mut rec, edits);
            }
            if doc.deleted().contains(&fid) {
                // The game's own way of removing a reference.
                rec.header.flags |= record_flags::DELETED;
            }
            remap_record(&mut rec, schema, &override_map, &mut report, &mut problems);
            let cell = cell_of_record_lo(doc, lo, schema, fid);
            override_record(&mut nodes, lo, schema, &override_map, cell, &rec);
            overrides.push(rec.header.form_id);
            report.records += 1;
        }
        if !overrides.is_empty() {
            add_onam(&mut nodes, &overrides)?;
        }
    } else {
        // Standard mod editing mode: doc is the mod itself.
        // Keep doc's own records and apply any edits or deletes.
        report.records = 0;
        let mut walk = |node: &mut Node| {
            if let Node::Record(r) = node {
                if &r.header.sig != b"TES4" {
                    report.records += 1;
                    let fid = r.header.form_id;
                    if doc.deleted().contains(&fid) {
                        r.header.flags |= record_flags::DELETED;
                    }
                    if let Some(edits) = doc.edits_for(fid) {
                        crate::document::apply_edits_pub(r, edits);
                    }
                    remap_record(r, schema, &map, &mut report, &mut problems);
                }
            }
        };
        for n in &mut nodes {
            walk_node(n, &mut walk);
        }

        // Overrides of records from masters
        let mut overrides: Vec<u32> = Vec::new();
        let mut touched: Vec<u32> = doc.deleted().to_vec();
        touched.extend(doc.edits().map(|(f, _)| *f));
        touched.sort_unstable();
        touched.dedup();

        for fid in touched {
            if find_record(doc.nodes(), fid).is_some() {
                continue;
            }

            if let Some(lo) = lo {
                if let Some((pi, orig)) = fnv_world::fields::winner(lo, fid) {
                    let orig_name = &lo.plugins[pi].name;
                    if let Some(m_idx) = masters
                        .names
                        .iter()
                        .position(|n| n.eq_ignore_ascii_case(orig_name))
                    {
                        let target_fid = ((m_idx as u32) << 24) | (fid & 0x00FF_FFFF);
                        let mut rec = orig.clone();
                        rec.header.form_id = target_fid;
                        if doc.deleted().contains(&fid) {
                            rec.header.flags |= record_flags::DELETED;
                        }
                        if let Some(edits) = doc.edits_for(fid) {
                            crate::document::apply_edits_pub(&mut rec, edits);
                        }
                        remap_record(&mut rec, schema, &map, &mut report, &mut problems);
                        let cell = cell_of_record_lo(doc, Some(lo), schema, fid);
                        override_record(&mut nodes, Some(lo), schema, &map, cell, &rec);
                        overrides.push(target_fid);
                        report.records += 1;
                    } else {
                        problems.push(format!(
                            "record {fid:08X} belongs to {orig_name}, which is not in the master list"
                        ));
                    }
                } else {
                    problems.push(format!("record {fid:08X} not found in load order"));
                }
            } else {
                problems.push(format!(
                    "record {fid:08X} is from a master, but no load order was provided to resolve it"
                ));
            }
        }
        if !overrides.is_empty() {
            add_onam(&mut nodes, &overrides)?;
        }
    }

    // The session's inserts, with FormIDs allocated above the masters' range.
    report.inserted = doc.inserted().len();
    report.deleted = doc.deleted().len();

    // Allocate FormIDs for the session's inserts and append them.
    //
    // A staged record carries FormID 0, and the session's edits on it are keyed
    // on that placeholder, so they are applied to the record before it is
    // written. Only the first insert takes them: a second placement has no edits
    // queued under the same key, and applying the first record's edits to it
    // would be wrong.
    let mut placeholder_edits = doc.edits_for(0).unwrap_or(&[]).to_vec();
    for (next_id, p) in (allocate_base(doc, new_self)..).zip(doc.inserted()) {
        let mut rec = p.record.clone();
        if !placeholder_edits.is_empty() {
            crate::document::apply_edits_pub(&mut rec, &placeholder_edits);
            // One record's worth, in order.
            placeholder_edits.clear();
        }
        rec.header.form_id = next_id;
        remap_record(&mut rec, schema, &map, &mut report, &mut problems);
        insert_into_cell(&mut nodes, lo, schema, &map, p.cell, rec);
        report.records += 1;
    }
    report.unremapped = problems;

    let mut bytes = Vec::new();
    fnv_esm::raw::write_nodes(&nodes, &mut bytes);
    report.bytes = bytes.len();
    std::fs::write(out_path, &bytes).map_err(|e| e.to_string())?;
    Ok(report)
}

/// Replace the TES4 header record with one naming the new master list.
fn rewrite_header(nodes: &mut [Node], masters: &MasterList, new_self: u32) -> Result<(), String> {
    let Some(Node::Record(tes4)) = nodes.first_mut() else {
        return Err("plugin does not start with a TES4 record".into());
    };
    let mut subs = tes4
        .subrecords()
        .map_err(|e| format!("TES4 header will not parse: {e}"))?;

    // Keep HEDR, CNAM and SNAM; drop the old MAST/DATA pairs and ONAM.
    let mut out: Vec<Subrecord> = Vec::new();
    for s in subs.drain(..) {
        match &s.sig {
            b"MAST" | b"DATA" | b"ONAM" => {}
            b"HEDR" => {
                // next_object_id starts above the masters' FormID range.
                let mut data = s.data.clone();
                if data.len() >= 12 {
                    data[8..12].copy_from_slice(&first_free_id(new_self).to_le_bytes());
                }
                out.push(Subrecord {
                    sig: s.sig,
                    data,
                    extended: s.extended,
                });
            }
            _ => out.push(s),
        }
    }
    // The new master list, as MAST/DATA pairs.
    for name in &masters.names {
        out.push(Subrecord {
            sig: *b"MAST",
            data: crate::document::encode_zstring(name),
            extended: false,
        });
        out.push(Subrecord {
            sig: *b"DATA",
            data: 0u64.to_le_bytes().to_vec(),
            extended: false,
        });
    }
    if let Some(a) = &masters.author {
        out.push(Subrecord {
            sig: *b"CNAM",
            data: crate::document::encode_zstring(a),
            extended: false,
        });
    }
    if let Some(d) = &masters.description {
        out.push(Subrecord {
            sig: *b"SNAM",
            data: crate::document::encode_zstring(d),
            extended: false,
        });
    }
    // The plugin is not a master file.
    tes4.header.flags &= !record_flags::MASTER;
    tes4.header.data_size = fnv_esm::raw::write_subrecords(&out).len() as u32;
    tes4.raw = fnv_esm::raw::write_subrecords(&out).into();
    Ok(())
}

/// The first FormID index the new plugin's own records may use.
fn first_free_id(new_self: u32) -> u32 {
    new_self << 24
}

/// Where FormID allocation continues from for this session's inserts.
///
/// A staged record's FormID is a **placeholder**, not a handed-out one: the save
/// assigns the real FormID here. Counting the placeholder would push every insert
/// one past where it belongs (a record staged as `0x01000000` would be saved as
/// `0x01000001`), so only IDs that are already in the plugin's own range count.
fn allocate_base(doc: &crate::Document, new_self: u32) -> u32 {
    let base = first_free_id(new_self);
    let mut next = base;
    for p in doc.inserted() {
        // A staged placeholder is 0; anything else was handed out by a caller
        // that knows the numbering, and is left above.
        if p.record.header.form_id >= base {
            next = next.max(p.record.header.form_id + 1);
        }
    }
    next
}

/// Rewrite the FormIDs of one record's subrecords in place.
fn remap_record(
    r: &mut Record,
    schema: &Schema,
    map: &HashMap<u32, u32>,
    report: &mut SaveReport,
    problems: &mut Vec<String>,
) {
    let Ok(subs) = r.subrecords() else { return };
    let mut changed_any = false;
    let mut new_subs = Vec::with_capacity(subs.len());
    for s in &subs {
        // The top byte is the master index; the low 24 bits pass through.
        let remapped = crate::formid::remap_sub(schema, &r.header.sig, s, &|local| {
            let idx = local >> 24;
            let low = local & 0x00FF_FFFF;
            map.get(&idx).map(|new_idx| (new_idx << 24) | low)
        });
        match remapped {
            Some(new_data) => {
                if new_data != s.data {
                    changed_any = true;
                    report.remapped_subs += 1;
                    report.remapped_formids += count_changed(s, &new_data);
                }
                new_subs.push(Subrecord {
                    sig: s.sig,
                    data: new_data,
                    extended: s.extended,
                });
            }
            None => {
                // Left exactly as stored; the save says so.
                if problems.len() < 50 {
                    problems.push(format!(
                        "{} {:08X} {}/{}",
                        r.sig_str(),
                        r.header.form_id,
                        r.sig_str(),
                        s.sig_str()
                    ));
                }
                new_subs.push(s.clone());
            }
        }
    }
    if changed_any {
        let payload = fnv_esm::raw::write_subrecords(&new_subs);
        // A compressed record stores `u32 decompressed size` + zlib stream. The
        // subrecords were decoded from the decompressed form, so writing them
        // back uncompressed would leave the COMPRESSED flag set on plain bytes
        // and make the record unreadable. Recompress, and keep the stored size.
        r.raw = if r.is_compressed() {
            let mut v = (payload.len() as u32).to_le_bytes().to_vec();
            v.extend_from_slice(&fnv_inflate::zlib_store(&payload));
            v.into()
        } else {
            payload.into()
        };
    }
}

/// Drop every record from the tree, keeping the group structure the new records
/// hang from. A mod carries its own records; copying the master's would make the
/// load order read it as overriding all of them.
///
/// The TES4 header (`nodes[0]`) is a record and is kept: it is rewritten
/// separately by [`rewrite_header`], and a plugin without it is not loadable.
fn prune_records(nodes: &mut Vec<Node>) {
    for n in nodes.iter_mut() {
        if let Node::Group(g) = n {
            prune_records(&mut g.children);
        }
    }
    let keep_header = matches!(nodes.first(), Some(Node::Record(r)) if &r.header.sig == b"TES4");
    if keep_header {
        let header = nodes.remove(0);
        nodes.retain(|n| matches!(n, Node::Group(_)));
        nodes.insert(0, header);
    } else {
        nodes.retain(|n| matches!(n, Node::Group(_)));
    }
}

/// Find a record by FormID.
fn find_record(nodes: &[Node], form_id: u32) -> Option<&Record> {
    for n in nodes {
        match n {
            Node::Record(r) if r.header.form_id == form_id => return Some(r),
            Node::Group(g) => {
                if let Some(found) = find_record(&g.children, form_id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

fn cell_of_record_lo(
    doc: &crate::Document,
    lo: Option<&fnv_esm::LoadOrder<'_>>,
    schema: &Schema,
    form_id: u32,
) -> Option<u32> {
    if let Some(lo) = lo {
        let ix = fnv_world::WorldIndex::build(lo, schema);
        if let Some(e) = ix.refs.get(&form_id) {
            return Some(e.cell);
        }
    }
    cell_of_record(doc, form_id)
}

/// The cell a record lives in, from the load order's own index.
///
/// `fnv_world::WorldIndex` already maps every FormID to its cell
/// (`ix.refs[fid].cell`), and it is the same index the renderer and the
/// simulation use to decide which references exist and where. Guessing the cell
/// from the FormID's low bits would be wrong: a REFR FormID has no relation to
/// its cell's FormID.
fn cell_of_record(doc: &crate::Document, form_id: u32) -> Option<u32> {
    // The index is built over the plugin being edited on its own, which is
    // enough to say where the record sits inside it.
    let lo = fnv_esm::LoadOrder::new(std::slice::from_ref(doc.plugin())).ok()?;
    let ix = fnv_world::WorldIndex::build(&lo, &Schema::default());
    ix.refs.get(&form_id).map(|e| e.cell)
}

fn ensure_record_present(
    children: &mut Vec<Node>,
    lo: Option<&fnv_esm::LoadOrder<'_>>,
    schema: &Schema,
    map: &HashMap<u32, u32>,
    form_id: u32,
    sig: [u8; 4],
) {
    if children.iter().any(|n| match n {
        Node::Record(r) => r.header.form_id == form_id && r.header.sig == sig,
        _ => false,
    }) {
        return;
    }
    if let Some(lo) = lo {
        if let Some((_, orig)) = fnv_world::fields::winner(lo, form_id) {
            let mut rec = orig.clone();
            let mut problems = Vec::new();
            let mut report = SaveReport::default();
            remap_record(&mut rec, schema, map, &mut report, &mut problems);
            let insert_pos = children
                .iter()
                .position(|n| matches!(n, Node::Group(_)))
                .unwrap_or(children.len());
            children.insert(insert_pos, Node::Record(rec));
        }
    }
}

fn find_or_create_child_group(
    children: &mut Vec<Node>,
    group_type: i32,
    label: [u8; 4],
) -> &mut fnv_esm::raw::Group {
    let pos = match children.iter().position(|n| match n {
        Node::Group(g) => g.header.group_type == group_type && g.header.label == label,
        _ => false,
    }) {
        Some(idx) => idx,
        None => {
            children.push(Node::Group(fnv_esm::raw::Group {
                header: fnv_esm::raw::GroupHeader {
                    size: 0,
                    label,
                    group_type,
                    stamp: 0,
                    unknown1: 0,
                    version: 0,
                    unknown2: 0,
                },
                children: Vec::new(),
                offset: 0,
            }));
            children.len() - 1
        }
    };
    let Node::Group(g) = &mut children[pos] else {
        unreachable!()
    };
    g
}

fn ensure_cell_hierarchy_and_insert(
    nodes: &mut Vec<Node>,
    lo: Option<&fnv_esm::LoadOrder<'_>>,
    schema: &Schema,
    map: &HashMap<u32, u32>,
    cell_id: u32,
    rec: Record,
) {
    let (world_id, grid, is_interior, is_persistent) = if let Some(lo) = lo {
        let ix = fnv_world::WorldIndex::build(lo, schema);
        let cell_entry = ix.cells.get(&cell_id);
        let world = cell_entry.and_then(|c| c.world);
        let grid = cell_entry.and_then(|c| c.grid);
        let interior = cell_entry
            .map(|c| c.is_interior())
            .unwrap_or(world.is_none());
        let persistent = (rec.header.flags & 0x400 != 0)
            || ix
                .refs
                .get(&rec.header.form_id)
                .is_some_and(|r| r.persistent);
        (world, grid, interior, persistent)
    } else {
        (None, None, true, rec.header.flags & 0x400 != 0)
    };

    let target_child_type = if is_persistent {
        fnv_esm::raw::group_type::CELL_PERSISTENT_CHILDREN
    } else {
        fnv_esm::raw::group_type::CELL_TEMPORARY_CHILDREN
    };

    if let (false, Some(world_fid)) = (is_interior, world_id) {
        // 1. TOP: WRLD
        let top_wrld = find_or_create_child_group(nodes, fnv_esm::raw::group_type::TOP, *b"WRLD");
        // Ensure WRLD record is present
        ensure_record_present(&mut top_wrld.children, lo, schema, map, world_fid, *b"WRLD");
        // 2. WORLD_CHILDREN (type 1, label: world_fid)
        let world_children = find_or_create_child_group(
            &mut top_wrld.children,
            fnv_esm::raw::group_type::WORLD_CHILDREN,
            world_fid.to_le_bytes(),
        );

        if let Some((gx, gy)) = grid {
            // Grid cell:
            // 3. EXTERIOR_CELL_BLOCK (type 4, label: [block_y as i16, block_x as i16])
            let bx = (gx >> 5) as i16;
            let by = (gy >> 5) as i16;
            let mut block_label = [0u8; 4];
            block_label[0..2].copy_from_slice(&by.to_le_bytes());
            block_label[2..4].copy_from_slice(&bx.to_le_bytes());
            let block_g = find_or_create_child_group(
                &mut world_children.children,
                fnv_esm::raw::group_type::EXTERIOR_CELL_BLOCK,
                block_label,
            );

            // 4. EXTERIOR_CELL_SUBBLOCK (type 5, label: [subblock_y as i16, subblock_x as i16])
            let sx = (gx >> 3) as i16;
            let sy = (gy >> 3) as i16;
            let mut sub_label = [0u8; 4];
            sub_label[0..2].copy_from_slice(&sy.to_le_bytes());
            sub_label[2..4].copy_from_slice(&sx.to_le_bytes());
            let sub_g = find_or_create_child_group(
                &mut block_g.children,
                fnv_esm::raw::group_type::EXTERIOR_CELL_SUBBLOCK,
                sub_label,
            );

            // Ensure CELL record is present
            ensure_record_present(&mut sub_g.children, lo, schema, map, cell_id, *b"CELL");

            // 5. CELL_CHILDREN (type 6, label: cell_id)
            let cell_children = find_or_create_child_group(
                &mut sub_g.children,
                fnv_esm::raw::group_type::CELL_CHILDREN,
                cell_id.to_le_bytes(),
            );

            // 6. CELL_TEMPORARY_CHILDREN (type 9) or PERSISTENT (type 8)
            let leaf_g = find_or_create_child_group(
                &mut cell_children.children,
                target_child_type,
                cell_id.to_le_bytes(),
            );
            leaf_g.children.push(Node::Record(rec));
        } else {
            // Persistent cell directly in world_children:
            ensure_record_present(
                &mut world_children.children,
                lo,
                schema,
                map,
                cell_id,
                *b"CELL",
            );
            let cell_children = find_or_create_child_group(
                &mut world_children.children,
                fnv_esm::raw::group_type::CELL_CHILDREN,
                cell_id.to_le_bytes(),
            );
            let leaf_g = find_or_create_child_group(
                &mut cell_children.children,
                target_child_type,
                cell_id.to_le_bytes(),
            );
            leaf_g.children.push(Node::Record(rec));
        }
    } else {
        // Interior cell
        // 1. TOP: CELL
        let top_cell = find_or_create_child_group(nodes, fnv_esm::raw::group_type::TOP, *b"CELL");
        // 2. INTERIOR_CELL_BLOCK (type 2, label: cell_id % 10)
        let bnum = (cell_id % 10) as i32;
        let block_g = find_or_create_child_group(
            &mut top_cell.children,
            fnv_esm::raw::group_type::INTERIOR_CELL_BLOCK,
            bnum.to_le_bytes(),
        );
        // 3. INTERIOR_CELL_SUBBLOCK (type 3, label: (cell_id / 10) % 10)
        let sbnum = ((cell_id / 10) % 10) as i32;
        let sub_g = find_or_create_child_group(
            &mut block_g.children,
            fnv_esm::raw::group_type::INTERIOR_CELL_SUBBLOCK,
            sbnum.to_le_bytes(),
        );
        // Ensure CELL record is present
        ensure_record_present(&mut sub_g.children, lo, schema, map, cell_id, *b"CELL");
        // 4. CELL_CHILDREN (type 6, label: cell_id)
        let cell_children = find_or_create_child_group(
            &mut sub_g.children,
            fnv_esm::raw::group_type::CELL_CHILDREN,
            cell_id.to_le_bytes(),
        );
        // 5. CELL_TEMPORARY_CHILDREN (type 9) or PERSISTENT (type 8)
        let leaf_g = find_or_create_child_group(
            &mut cell_children.children,
            target_child_type,
            cell_id.to_le_bytes(),
        );
        leaf_g.children.push(Node::Record(rec));
    }
}

/// Place an overriding record into the group the original record lives in.
///
/// An override has to sit in the same cell-children group as the record it
/// replaces, or the load order will not associate the two. The cell comes from
/// [`cell_of_record`], and the group is found by that cell's FormID.
fn override_record(
    nodes: &mut Vec<Node>,
    lo: Option<&fnv_esm::LoadOrder<'_>>,
    schema: &Schema,
    map: &HashMap<u32, u32>,
    cell: Option<u32>,
    rec: &Record,
) {
    let sig = rec.header.sig;
    // A placed reference goes into its own cell's children groups.
    if let Some(cell) = cell {
        if let Some(g) = find_cell_children_mut(nodes, cell) {
            let is_persistent = rec.header.flags & 0x400 != 0;
            let target_type = if is_persistent {
                fnv_esm::raw::group_type::CELL_PERSISTENT_CHILDREN
            } else {
                fnv_esm::raw::group_type::CELL_TEMPORARY_CHILDREN
            };
            let leaf = find_or_create_child_group(&mut g.children, target_type, cell.to_le_bytes());
            leaf.children.push(Node::Record(rec.clone()));
            return;
        }
        ensure_cell_hierarchy_and_insert(nodes, lo, schema, map, cell, rec.clone());
        return;
    }
    // Everything else keeps its own top-level group, labelled with the record
    // signature.
    if let Some(Node::Group(g)) = nodes.iter_mut().find(|n| match n {
        Node::Group(g) => {
            g.header.group_type == fnv_esm::raw::group_type::TOP && g.header.label == sig
        }
        _ => false,
    }) {
        g.children.push(Node::Record(rec.clone()));
        return;
    }
    // No group to hang it from: put it in a fresh top-level group so the record
    // is at least present, and say so.
    tracing::warn!(
        record = %rec.sig_str(),
        form_id = format_args!("{:08X}", rec.header.form_id),
        ?cell,
        "no group for the overridden record; a new top-level group is created"
    );
    nodes.push(Node::Group(fnv_esm::raw::Group {
        header: fnv_esm::raw::GroupHeader {
            size: 0,
            label: sig,
            group_type: fnv_esm::raw::group_type::TOP,
            stamp: 0,
            unknown1: 0,
            version: 0,
            unknown2: 0,
        },
        children: vec![Node::Record(rec.clone())],
        offset: 0,
    }));
}

/// Declare the overridden FormIDs in the header's ONAM array.
///
/// The shipped masters store ONAM as **one packed subrecord** holding every
/// FormID (DeadMoney.esm: 179 FormIDs in 716 bytes), not one subrecord per
/// FormID. Emitting one per FormID would leave the reader seeing a single-entry
/// list and dropping the rest.
///
/// Without it the load order reads the records as new definitions that happen to
/// reuse a FormID, which is what made the whole-plugin save look like it was
/// overriding everything.
fn add_onam(nodes: &mut [Node], overrides: &[u32]) -> Result<(), String> {
    let Some(Node::Record(tes4)) = nodes.first_mut() else {
        return Err("plugin does not start with a TES4 record".into());
    };
    let mut subs = tes4
        .subrecords()
        .map_err(|e| format!("TES4 header will not parse: {e}"))?;
    // Replace any existing ONAM with the complete list, as one packed subrecord.
    subs.retain(|s| &s.sig != b"ONAM");
    let mut data = Vec::with_capacity(overrides.len() * 4);
    for &fid in overrides {
        data.extend_from_slice(&fid.to_le_bytes());
    }
    subs.push(Subrecord {
        sig: *b"ONAM",
        data,
        extended: false,
    });
    tes4.header.data_size = fnv_esm::raw::write_subrecords(&subs).len() as u32;
    tes4.raw = fnv_esm::raw::write_subrecords(&subs).into();
    Ok(())
}

/// A rough count of how many FormID-sized words differ between two payloads.
fn count_changed(before: &Subrecord, after: &[u8]) -> usize {
    let a = &before.data;
    let n = a.len().min(after.len());
    let mut words = 0;
    let mut i = 0;
    while i + 4 <= n {
        if a[i..i + 4] != after[i..i + 4] {
            words += 1;
        }
        i += 4;
    }
    words
}

/// Append `rec` to the cell's children group, creating the group tree
/// when the master has none for that cell.
fn insert_into_cell(
    nodes: &mut Vec<Node>,
    lo: Option<&fnv_esm::LoadOrder<'_>>,
    schema: &Schema,
    map: &HashMap<u32, u32>,
    cell: u32,
    rec: Record,
) {
    if let Some(g) = find_cell_children_mut(nodes, cell) {
        let is_persistent = rec.header.flags & 0x400 != 0;
        let target_type = if is_persistent {
            fnv_esm::raw::group_type::CELL_PERSISTENT_CHILDREN
        } else {
            fnv_esm::raw::group_type::CELL_TEMPORARY_CHILDREN
        };
        let leaf = find_or_create_child_group(&mut g.children, target_type, cell.to_le_bytes());
        leaf.children.push(Node::Record(rec));
        return;
    }
    ensure_cell_hierarchy_and_insert(nodes, lo, schema, map, cell, rec);
}

fn find_cell_children_mut(nodes: &mut [Node], cell: u32) -> Option<&mut fnv_esm::raw::Group> {
    for n in nodes {
        if let Node::Group(g) = n {
            if g.header.group_type == fnv_esm::raw::group_type::CELL_CHILDREN
                && g.header.label_u32() == cell
            {
                return Some(g);
            }
            if let Some(found) = find_cell_children_mut(&mut g.children, cell) {
                return Some(found);
            }
        }
    }
    None
}

fn walk_node<F: FnMut(&mut Node)>(node: &mut Node, f: &mut F) {
    f(node);
    if let Node::Group(g) = node {
        for c in &mut g.children {
            walk_node(c, f);
        }
    }
}

/// A header suitable for a fresh plugin, for building a new mod from scratch.
pub fn empty_header(masters: &MasterList) -> PluginHeader {
    PluginHeader {
        flags: 0,
        form_version: 0,
        version: 1.34,
        num_records: 0,
        next_object_id: first_free_id(masters.names.len() as u32),
        author: masters.author.clone(),
        description: masters.description.clone(),
        masters: masters
            .names
            .iter()
            .map(|n| fnv_esm::plugin::MasterRef {
                name: n.clone(),
                data: Some(0),
            })
            .collect(),
        overridden_forms: Vec::new(),
        other: Vec::new(),
    }
}
