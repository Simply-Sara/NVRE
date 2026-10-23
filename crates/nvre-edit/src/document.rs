//! A mutable view of one plugin: edit fields, insert and remove references, and
//! write the result back as a `.esp`.
//!
//! The unit of work is the [`Document`], which owns a parsed [`Plugin`] plus the
//! edits applied to it. Nothing is written to the game's folder: saving produces a
//! new file wherever the user asks.
//!
//! # Why a copy-on-write payload
//!
//! `nvre_esm::Record::raw` is a zero-copy slice of the memory-mapped file, so a
//! record cannot be edited in place. Every edit therefore replaces `raw` with an
//! owned `Bytes`; untouched records keep sharing the mapping and cost nothing.

use nvre_esm::raw::{record_flags, Node, Record, RecordHeader, Subrecord};
use nvre_esm::schema::Schema;
use nvre_esm::{EsmError, Plugin};
use serde_json::Value;
use std::collections::HashMap;

/// One pending change to a record's payload.
#[derive(Debug, Clone)]
pub enum Edit {
    /// Replace the value of the subrecord with this signature. When several
    /// subrecords share a signature, the first is edited (matching how the game
    /// reads them).
    SetSub { sig: [u8; 4], data: Vec<u8> },
    /// Replace all occurrences of subrecords with this signature with a new list of subrecords.
    SetSubList { sig: [u8; 4], data: Vec<Vec<u8>> },
    /// Replace all occurrences of subrecords matching `remove_sigs` with a sequential list of subrecords.
    SetSubrecordSequence {
        remove_sigs: Vec<[u8; 4]>,
        subs: Vec<([u8; 4], Vec<u8>)>,
    },
    /// Drop the subrecord with this signature.
    RemoveSub { sig: [u8; 4] },
    /// Replace the record header's flags (deleted, persistent, ...).
    SetFlags(u32),
    /// Replace the record header's FormID.
    SetFormId(u32),
}

/// A record identified by its FormID and where it lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordId {
    pub form_id: u32,
}

/// An editable plugin: the parsed structure plus the edits applied so far.
pub struct Document {
    plugin: Plugin,
    /// Payload overrides by FormID, applied on save.
    edits: HashMap<u32, Vec<Edit>>,
    /// Records to drop entirely, by FormID.
    removed: Vec<u32>,
    /// Records to add: (group path to insert into, record).
    added: Vec<PendingRecord>,
    /// Cells staged for creation, with the group tree they need.
    added_cells: Vec<PendingCell>,
    /// Worldspaces staged for creation.
    added_worlds: Vec<PendingWorldspace>,
    /// FormID → path into `plugin.nodes`, so a record can be found without
    /// walking the tree. Built once, when the plugin is opened.
    index: HashMap<u32, RecordPath>,
}

/// A path to a record: the group chain it sits in, then its own index.
///
/// The plugin's records live in a tree of groups, so a lookup has to return the
/// slice a record belongs to rather than the record itself, to let a caller take
/// `&Record` out of `&self`.
type RecordPath = Vec<usize>;

/// A record to insert, with the cell it belongs to.
#[derive(Debug, Clone)]
pub struct PendingRecord {
    /// The CELL FormID whose temporary-children group receives this record.
    pub cell: u32,
    pub record: Record,
}

/// A cell staged for creation.
///
/// The FormIDs are handed out when the cell is staged, not when it is saved,
/// because a cell's place in the group tree depends on its own FormID: the
/// interior block and sub-block labels are computed from it.
#[derive(Debug, Clone)]
pub struct PendingCell {
    /// What the cell is: its Editor ID, its worldspace and its grid.
    pub spec: crate::cell::NewCell,
    /// The FormID the `CELL` record will carry.
    pub form_id: u32,
    /// The FormID the `LAND` record will carry, when the cell has terrain.
    pub land_form_id: Option<u32>,
}

/// A worldspace staged for creation.
#[derive(Debug, Clone)]
pub struct PendingWorldspace {
    /// What the worldspace is: its Editor ID, its heights and its climate.
    pub spec: crate::cell::NewWorldspace,
    /// The FormID the `WRLD` record will carry.
    pub form_id: u32,
}

impl Document {
    /// Open a plugin for editing (read-only until saved).
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Document, EsmError> {
        let plugin = Plugin::open(path)?;
        let index = build_index(&plugin.nodes);
        Ok(Document {
            plugin,
            edits: HashMap::new(),
            removed: Vec::new(),
            added: Vec::new(),
            added_cells: Vec::new(),
            added_worlds: Vec::new(),
            index,
        })
    }

    /// Wrap an already-parsed plugin.
    pub fn from_plugin(plugin: Plugin) -> Document {
        let index = build_index(&plugin.nodes);
        Document {
            plugin,
            edits: HashMap::new(),
            removed: Vec::new(),
            added: Vec::new(),
            added_cells: Vec::new(),
            added_worlds: Vec::new(),
            index,
        }
    }

    /// Create a new, blank plugin (.esp or .esm) with the specified name and masters.
    pub fn create_empty(
        name: &str,
        is_master: bool,
        masters: &[String],
    ) -> Result<Document, EsmError> {
        let mut subs = Vec::new();
        // HEDR subrecord: version (1.34), num_records (0), next_object_id (0x00000800)
        let mut hedr = Vec::new();
        hedr.extend_from_slice(&1.34f32.to_le_bytes());
        hedr.extend_from_slice(&0i32.to_le_bytes());
        hedr.extend_from_slice(&0x800u32.to_le_bytes());
        subs.push(nvre_esm::raw::Subrecord {
            sig: *b"HEDR",
            data: hedr,
            extended: false,
        });

        // CNAM author
        subs.push(nvre_esm::raw::Subrecord {
            sig: *b"CNAM",
            data: b"NVRE\0".to_vec(),
            extended: false,
        });

        for m in masters {
            let mut n = m.as_bytes().to_vec();
            if !n.ends_with(&[0]) {
                n.push(0);
            }
            subs.push(nvre_esm::raw::Subrecord {
                sig: *b"MAST",
                data: n,
                extended: false,
            });
            subs.push(nvre_esm::raw::Subrecord {
                sig: *b"DATA",
                data: vec![0u8; 8],
                extended: false,
            });
        }

        let payload = nvre_esm::raw::write_subrecords(&subs);

        let flags = if is_master {
            nvre_esm::raw::record_flags::MASTER
        } else {
            0
        };

        let mut record_bytes = Vec::new();
        record_bytes.extend_from_slice(b"TES4");
        record_bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        record_bytes.extend_from_slice(&flags.to_le_bytes());
        record_bytes.extend_from_slice(&0u32.to_le_bytes()); // form_id
        record_bytes.extend_from_slice(&0u32.to_le_bytes()); // vc_info
        record_bytes.extend_from_slice(&15u16.to_le_bytes()); // form_version
        record_bytes.extend_from_slice(&0u16.to_le_bytes()); // vc_info2
        record_bytes.extend_from_slice(&payload);

        let plugin = Plugin::parse(&record_bytes, name.to_string())?;
        Ok(Document::from_plugin(plugin))
    }

    pub fn plugin(&self) -> &Plugin {
        &self.plugin
    }

    /// The plugin's node tree, for reading records out of it.
    pub fn nodes(&self) -> &[nvre_esm::raw::Node] {
        &self.plugin.nodes
    }

    pub fn name(&self) -> &str {
        &self.plugin.name
    }

    pub fn is_master(&self) -> bool {
        self.plugin.header.is_master()
    }

    /// Whether the plugin's strings are string IDs rather than text.
    ///
    /// A localized plugin's `FULL` is a 4-byte string ID into its string
    /// tables, so a record's name is resolved through them rather than read
    /// from the payload.
    pub fn is_localized(&self) -> bool {
        self.plugin.header.is_localized()
    }

    /// Every record in file order, with its FormID.
    pub fn records(&self) -> Vec<(u32, &Record)> {
        let mut out = Vec::new();
        nvre_esm::raw::visit_nodes(&self.plugin.nodes, &mut |r, _| {
            out.push((r.header.form_id, r))
        });
        out
    }

    /// The record with this FormID, if the plugin defines it.
    ///
    /// Constant time, via the index built at load: a plugin's records live in a
    /// tree of groups, so the path down to each one is recorded and walked, rather
    /// than the whole tree. The editor reads hundreds of records a frame, so the
    /// difference is the editor working or not.
    pub fn record(&self, form_id: u32) -> Option<&Record> {
        let path = self.index.get(&form_id)?;
        let mut nodes: &[Node] = &self.plugin.nodes;
        // Every step but the last is a group; the last is the record's own index.
        for &i in &path[..path.len() - 1] {
            match nodes.get(i)? {
                Node::Group(g) => nodes = &g.children,
                Node::Record(_) => return None,
            }
        }
        match nodes.get(*path.last()?)? {
            Node::Record(r) => Some(r),
            Node::Group(_) => None,
        }
    }

    /// Queue a subrecord edit. `data` must be the encoded payload.
    pub fn set_sub(&mut self, form_id: u32, sig: &[u8; 4], data: Vec<u8>) {
        let edits = self.edits.entry(form_id).or_default();
        if let Some(existing) = edits
            .iter_mut()
            .find(|e| matches!(e, Edit::SetSub { sig: s, .. } if s == sig))
        {
            *existing = Edit::SetSub { sig: *sig, data };
        } else {
            edits.push(Edit::SetSub { sig: *sig, data });
        }
    }

    /// Queue a subrecord removal.
    pub fn remove_sub(&mut self, form_id: u32, sig: &[u8; 4]) {
        self.edits
            .entry(form_id)
            .or_default()
            .push(Edit::RemoveSub { sig: *sig });
    }

    /// Queue replacing all subrecords with `sig` with the given list of buffers.
    pub fn set_sub_list(&mut self, form_id: u32, sig: &[u8; 4], data: Vec<Vec<u8>>) {
        self.edits
            .entry(form_id)
            .or_default()
            .push(Edit::SetSubList { sig: *sig, data });
    }

    /// Queue replacing all subrecords matching `remove_sigs` with a sequential list of subrecords.
    pub fn set_sub_sequence(
        &mut self,
        form_id: u32,
        remove_sigs: Vec<[u8; 4]>,
        subs: Vec<([u8; 4], Vec<u8>)>,
    ) {
        self.edits
            .entry(form_id)
            .or_default()
            .push(Edit::SetSubrecordSequence { remove_sigs, subs });
    }

    /// Queue a header-flag change.
    pub fn set_flags(&mut self, form_id: u32, flags: u32) {
        self.edits
            .entry(form_id)
            .or_default()
            .push(Edit::SetFlags(flags));
    }

    /// Queue a FormID change.
    pub fn set_form_id(&mut self, from: u32, to: u32) {
        self.edits
            .entry(from)
            .or_default()
            .push(Edit::SetFormId(to));
    }

    /// Queue a record for deletion (it is written with the Deleted flag set, the
    /// way the game itself removes a reference, rather than dropped from the file).
    pub fn delete_record(&mut self, form_id: u32) {
        if !self.removed.contains(&form_id) {
            self.removed.push(form_id);
        }
    }

    /// Queue a new record into a cell's temporary-children group.
    pub fn insert(&mut self, pending: PendingRecord) {
        self.added.push(pending);
    }

    /// The records queued for insertion this session.
    pub fn inserted(&self) -> &[PendingRecord] {
        &self.added
    }

    /// Stage a new cell.
    ///
    /// `global_form_id` is the FormID the cell will have in the load order —
    /// the session's own numbering, with the plugin being edited in the top
    /// byte. The save moves it into the new plugin's numbering, and the low 24
    /// bits are what the cell's group tree is built from: an interior cell's
    /// block and sub-block labels are computed from them.
    pub fn add_cell(&mut self, spec: crate::cell::NewCell, global_form_id: u32) -> u32 {
        let land_form_id = if spec.with_land {
            Some(global_form_id.wrapping_add(1))
        } else {
            None
        };
        self.added_cells.push(PendingCell {
            spec,
            form_id: global_form_id,
            land_form_id,
        });
        global_form_id
    }

    /// Stage a new worldspace.
    ///
    /// `global_form_id` is the FormID the worldspace will have in the load
    /// order; see [`Document::add_cell`].
    pub fn add_worldspace(&mut self, spec: crate::cell::NewWorldspace, global_form_id: u32) -> u32 {
        self.added_worlds.push(PendingWorldspace {
            spec,
            form_id: global_form_id,
        });
        global_form_id
    }

    /// The cells staged for creation.
    pub fn added_cells(&self) -> &[PendingCell] {
        &self.added_cells
    }

    /// The worldspaces staged for creation.
    pub fn added_worldspaces(&self) -> &[PendingWorldspace] {
        &self.added_worlds
    }

    /// The records queued for deletion this session.
    pub fn deleted(&self) -> &[u32] {
        &self.removed
    }

    /// Every queued edit, keyed by FormID.
    pub fn edits(&self) -> impl Iterator<Item = (&u32, &Vec<Edit>)> {
        self.edits.iter()
    }

    /// The edits queued for one FormID, if any.
    pub fn edits_for(&self, form_id: u32) -> Option<&[Edit]> {
        self.edits.get(&form_id).map(Vec::as_slice)
    }

    /// Whether anything is queued.
    pub fn dirty(&self) -> bool {
        !self.edits.is_empty() || !self.removed.is_empty() || !self.added.is_empty()
    }

    pub fn edit_count(&self) -> usize {
        self.edits.values().map(Vec::len).sum::<usize>() + self.removed.len() + self.added.len()
    }

    /// Apply every queued edit to a copy of the node tree and return the bytes.
    ///
    /// Untouched records keep their original payload, so the output is
    /// byte-identical to the input except where an edit actually changed
    /// something — the property the parity lab's `esm verify` checks.
    pub fn to_bytes(&self) -> Result<Vec<u8>, EsmError> {
        let mut nodes = self.plugin.nodes.clone();
        let mut applied = 0usize;
        let mut walk = |node: &mut Node| {
            if let Node::Record(r) = node {
                if let Some(edits) = self.edits.get(&r.header.form_id) {
                    apply_edits(r, edits);
                    applied += 1;
                }
                if self.removed.contains(&r.header.form_id) {
                    r.header.flags |= record_flags::DELETED;
                    applied += 1;
                }
            }
        };
        for n in &mut nodes {
            visit_mut(n, &mut walk);
        }
        insert_pending(&mut nodes, &self.added);
        tracing::debug!(
            records = applied,
            added = self.added.len(),
            "document encoded"
        );
        let mut out = Vec::new();
        nvre_esm::raw::write_nodes(&nodes, &mut out);
        Ok(out)
    }

    /// Write the edited plugin to `path`.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), EsmError> {
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

/// Apply queued edits to a record in place. Shared with [`crate::save`].
pub fn apply_edits_pub(r: &mut Record, edits: &[Edit]) {
    apply_edits(r, edits);
}

/// Depth-first walk over a node and its children, mutating records in place.
fn visit_mut<F: FnMut(&mut Node)>(node: &mut Node, f: &mut F) {
    f(node);
    if let Node::Group(g) = node {
        for c in &mut g.children {
            visit_mut(c, f);
        }
    }
}

fn apply_edits(r: &mut Record, edits: &[Edit]) {
    // Only touch the payload when a subrecord edit is queued: rebuilding the
    // subrecord list would re-serialize XXXX handling and could change bytes.
    let needs_subs = edits
        .iter()
        .any(|e| matches!(e, Edit::SetSub { .. } | Edit::RemoveSub { .. }));
    if needs_subs {
        match r.subrecords() {
            Ok(mut subs) => {
                for e in edits {
                    match e {
                        Edit::SetSub { sig, data } => {
                            if let Some(s) = subs.iter_mut().find(|s| &s.sig == sig) {
                                s.data = data.clone();
                            } else {
                                // A subrecord the record does not have yet: append it.
                                subs.push(Subrecord {
                                    sig: *sig,
                                    data: data.clone(),
                                    extended: false,
                                });
                            }
                        }
                        Edit::SetSubList { sig, data } => {
                            let first_idx = subs.iter().position(|s| &s.sig == sig);
                            subs.retain(|s| &s.sig != sig);
                            let insert_pos = first_idx.unwrap_or(subs.len());
                            for (offset, entry) in data.iter().enumerate() {
                                subs.insert(
                                    insert_pos + offset,
                                    Subrecord {
                                        sig: *sig,
                                        data: entry.clone(),
                                        extended: false,
                                    },
                                );
                            }
                        }
                        Edit::SetSubrecordSequence {
                            remove_sigs,
                            subs: new_subs,
                        } => {
                            let first_idx = subs.iter().position(|s| remove_sigs.contains(&s.sig));
                            subs.retain(|s| !remove_sigs.contains(&s.sig));
                            let insert_pos = first_idx.unwrap_or(subs.len());
                            for (offset, (sig, data)) in new_subs.iter().enumerate() {
                                subs.insert(
                                    insert_pos + offset,
                                    Subrecord {
                                        sig: *sig,
                                        data: data.clone(),
                                        extended: false,
                                    },
                                );
                            }
                        }
                        Edit::RemoveSub { sig } => subs.retain(|s| &s.sig != sig),
                        _ => {}
                    }
                }
                let payload = nvre_esm::raw::write_subrecords(&subs);
                // A compressed record stores `u32 decompressed size` + a zlib
                // stream. The subrecords were decoded from the decompressed
                // form, so writing them back uncompressed would leave the
                // COMPRESSED flag set on plain bytes and make the record
                // unreadable. Recompress, and keep the stored size.
                r.raw = if r.is_compressed() {
                    let mut v = (payload.len() as u32).to_le_bytes().to_vec();
                    v.extend_from_slice(&nvre_inflate::zlib_store(&payload));
                    v.into()
                } else {
                    payload.into()
                };
            }
            Err(e) => {
                // A payload that cannot be split into subrecords is left exactly as
                // it is: the editor refuses to guess, and reports on save.
                tracing::warn!(record = %r.sig_str(), form_id = format_args!("{:08X}", r.header.form_id), %e, "subrecords will not parse; record left unchanged");
            }
        }
    }
    for e in edits {
        match e {
            Edit::SetFlags(flags) => r.header.flags = *flags,
            Edit::SetFormId(fid) => r.header.form_id = *fid,
            _ => {}
        }
    }
}

/// Insert every pending record into its cell's temporary-children group.
///
/// The group tree is created when the cell has none yet: a plugin adding a
/// reference to a master's cell must be able to write it even though the master
/// only has a persistent-children group.
fn insert_pending(nodes: &mut [Node], pending: &[PendingRecord]) {
    if pending.is_empty() {
        return;
    }
    // Group the pending records by cell, keeping order.
    let mut by_cell: HashMap<u32, Vec<&Record>> = HashMap::new();
    for p in pending {
        by_cell.entry(p.cell).or_default().push(&p.record);
    }
    for (cell, recs) in by_cell {
        let Some(group) = find_cell_children_mut(nodes, cell) else {
            tracing::warn!(
                cell = format_args!("{cell:08X}"),
                "no cell-children group for cell; record not inserted"
            );
            continue;
        };
        // The temporary-children group (type 9) is where new placements go.
        let temp = group.children.iter_mut().find_map(|c| match c {
            Node::Group(g)
                if g.header.group_type == nvre_esm::raw::group_type::CELL_TEMPORARY_CHILDREN =>
            {
                Some(g)
            }
            _ => None,
        });
        let temp = match temp {
            Some(g) => g,
            None => {
                group.children.push(Node::Group(nvre_esm::raw::Group {
                    header: nvre_esm::raw::GroupHeader {
                        size: 0,
                        label: [0; 4],
                        group_type: nvre_esm::raw::group_type::CELL_TEMPORARY_CHILDREN,
                        stamp: 0,
                        unknown1: 0,
                        version: 0,
                        unknown2: 0,
                    },
                    children: Vec::new(),
                    offset: 0,
                }));
                match group.children.last_mut() {
                    Some(Node::Group(g)) => g,
                    _ => unreachable!("just pushed a group"),
                }
            }
        };
        for r in recs {
            temp.children.push(Node::Record(r.clone()));
        }
    }
}

/// Find the cell-children group (type 6) of `cell` anywhere in the tree.
fn find_cell_children_mut(nodes: &mut [Node], cell: u32) -> Option<&mut nvre_esm::raw::Group> {
    for n in nodes {
        if let Node::Group(g) = n {
            if g.header.group_type == nvre_esm::raw::group_type::CELL_CHILDREN
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

/// Build a placed reference: the smallest valid REFR, as found in the masters.
///
/// `base` and `form_id` are plugin-local FormIDs. The position and rotation are
/// in game units and radians, as the DATA subrecord stores them.
///
/// `flags` is `0` by default, which is what 278,274 of the game's own placed
/// references in a temporary-children group carry (measured in FalloutNV.esm).
/// `0x400` (PERSISTENT) is only for references in a persistent-children group,
/// where 10,374 of them carry it — set it with [`make_refr_flags`] when the
/// reference belongs to a worldspace's persistent cell.
pub fn make_refr(
    form_id: u32,
    base: u32,
    position: [f32; 3],
    rotation: [f32; 3],
    edid: Option<&str>,
    scale: Option<f32>,
) -> Record {
    make_refr_flags(form_id, base, position, rotation, edid, scale, 0)
}

/// [`make_refr`] with explicit header flags.
pub fn make_refr_flags(
    form_id: u32,
    base: u32,
    position: [f32; 3],
    rotation: [f32; 3],
    edid: Option<&str>,
    scale: Option<f32>,
    flags: u32,
) -> Record {
    make_placed_record(
        *b"REFR", form_id, base, position, rotation, edid, scale, flags,
    )
}

/// Create a placed reference or actor record (`REFR`, `ACHR`, `ACRE`) with explicit header flags.
#[allow(clippy::too_many_arguments)]
pub fn make_placed_record(
    sig: [u8; 4],
    form_id: u32,
    base: u32,
    position: [f32; 3],
    rotation: [f32; 3],
    edid: Option<&str>,
    scale: Option<f32>,
    flags: u32,
) -> Record {
    let mut subs: Vec<Subrecord> = Vec::new();
    if let Some(e) = edid {
        subs.push(Subrecord {
            sig: *b"EDID",
            data: encode_zstring(e),
            extended: false,
        });
    }
    subs.push(Subrecord {
        sig: *b"NAME",
        data: base.to_le_bytes().to_vec(),
        extended: false,
    });
    if let Some(s) = scale {
        subs.push(Subrecord {
            sig: *b"XSCL",
            data: s.to_le_bytes().to_vec(),
            extended: false,
        });
    }
    let mut data = Vec::with_capacity(24);
    for v in position {
        data.extend_from_slice(&v.to_le_bytes());
    }
    for v in rotation {
        data.extend_from_slice(&v.to_le_bytes());
    }
    subs.push(Subrecord {
        sig: *b"DATA",
        data,
        extended: false,
    });
    let payload = nvre_esm::raw::write_subrecords(&subs);
    Record {
        header: RecordHeader {
            sig,
            data_size: payload.len() as u32,
            flags,
            form_id,
            // The game's own bookkeeping; 0 is accepted for new records.
            vc_info: 0,
            form_version: 15,
            vc_info2: 0,
        },
        raw: payload.into(),
        subrecords: Vec::new(),
        offset: 0,
    }
}

/// PERSISTENT, for references in a worldspace's persistent-children group.
pub const REFR_PERSISTENT: u32 = 0x400;

/// A NUL-terminated Windows-1252 Editor ID.
pub fn encode_zstring(s: &str) -> Vec<u8> {
    let mut v = nvre_util::cp1252::encode(s).unwrap_or_else(|| s.bytes().collect());
    v.push(0);
    v
}

/// Read a record's EDID, if present.
pub fn edid_of(r: &Record) -> Option<String> {
    r.subrecords()
        .ok()?
        .iter()
        .find(|s| &s.sig == b"EDID")
        .map(|s| nvre_util::cp1252::decode_z(&s.data))
}

/// Look up a record by FormID in a parsed plugin.
///
/// Walks the whole tree, so it is fine for one-off lookups but not for a loop —
/// use [`Document::record`] for that.
pub fn record_of(plugin: &Plugin, form_id: u32) -> Option<&Record> {
    let mut found = None;
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        if r.header.form_id == form_id && found.is_none() {
            found = Some(r);
        }
    });
    found
}

/// A summary row for the record browser.
#[derive(Debug, Clone)]
pub struct RecordRow {
    pub form_id: u32,
    pub sig: String,
    pub edid: Option<String>,
    pub full: Option<String>,
    pub flags: u32,
    pub form_version: u16,
}

/// Every record, as browser rows (EDID and FULL read straight from the payloads).
///
/// `strings` is this plugin's string table when it is localized — see
/// [`nvre_esm::strings`]. A localized `FULL` is a string ID rather than
/// text, so without the table its `full` is `None` rather than four bytes
/// of decoded noise.
pub fn rows(plugin: &Plugin, strings: Option<&nvre_esm::strings::StringTable>) -> Vec<RecordRow> {
    let mut out = Vec::new();
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        let (edid, full) = nvre_esm::inventory::names(r, strings).unwrap_or((None, None));
        out.push(RecordRow {
            form_id: r.header.form_id,
            sig: r.sig_str(),
            edid,
            full,
            flags: r.header.flags,
            form_version: r.header.form_version,
        });
    });
    out
}

/// The decoded value of one subrecord, for the property grid.
pub fn value_of(schema: &Schema, rec: &Record, sig: &[u8; 4]) -> Option<Value> {
    let sub = rec.subrecords().ok()?.into_iter().find(|s| &s.sig == sig)?;
    let mut ctx = nvre_esm::decode::DecodeCtx::new(schema, rec.header.sig);
    let d = nvre_esm::decode::decode_sub(&rec.header.sig, &sub, &mut ctx);
    match d.status {
        nvre_esm::decode::SubStatus::Ok => d.value,
        _ => None,
    }
}

/// FormID → path, for every record in the tree.
///
/// One walk at load time buys every later lookup. A first-wins rule keeps a
/// FormID pointing at the record the game would see, which is the one furthest
/// up the tree in visit order.
fn build_index(nodes: &[Node]) -> HashMap<u32, RecordPath> {
    let mut index = HashMap::new();
    fn go(nodes: &[Node], path: &mut RecordPath, index: &mut HashMap<u32, RecordPath>) {
        for (i, n) in nodes.iter().enumerate() {
            path.push(i);
            match n {
                Node::Record(r) => {
                    index
                        .entry(r.header.form_id)
                        .or_insert_with(|| path.clone());
                }
                Node::Group(g) => go(&g.children, path, index),
            }
            path.pop();
        }
    }
    go(nodes, &mut Vec::new(), &mut index);
    index
}
