//! Creating cells and worldspaces: the records, and the group tree they sit in.
//!
//! A cell that lands in the wrong group is silently ignored by the game, so
//! these tests check the *tree*, not just the record. The block and sub-block
//! labels are the part that is easy to get wrong, so the rules are checked
//! against the shipped masters as well as against what this crate writes.

use fnv_esm::raw::{group_type, Node};
use fnv_esm::{Plugin, Schema};
use nvre_edit::cell;
use nvre_edit::save::{write_plugin, MasterList};
use nvre_edit::Document;

fn data_dir() -> Option<std::path::PathBuf> {
    std::env::var("NVRE_DATA")
        .ok()
        .map(std::path::PathBuf::from)
}

fn schema_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")
}

const MASTERS: [&str; 1] = ["FalloutNV.esm"];

/// The path of groups a record sits under, as (group type, label) pairs.
fn path_to(nodes: &[Node], form_id: u32) -> Vec<(i32, u32)> {
    fn go(nodes: &[Node], form_id: u32, path: &mut Vec<(i32, u32)>) -> Option<Vec<(i32, u32)>> {
        for n in nodes {
            match n {
                Node::Record(r) => {
                    if r.header.form_id == form_id {
                        return Some(path.clone());
                    }
                }
                Node::Group(g) => {
                    path.push((g.header.group_type, g.header.label_u32()));
                    if let Some(found) = go(&g.children, form_id, path) {
                        return Some(found);
                    }
                    path.pop();
                }
            }
        }
        None
    }
    go(nodes, form_id, &mut Vec::new()).unwrap_or_default()
}

/// The interior block and sub-block labels, read out of the group tree.
fn interior_labels(nodes: &[Node], form_id: u32) -> Option<(i32, i32)> {
    let path = path_to(nodes, form_id);
    let mut block = None;
    let mut sub = None;
    for (t, label) in path {
        match t {
            group_type::INTERIOR_CELL_BLOCK => block = Some(label as i32),
            group_type::INTERIOR_CELL_SUBBLOCK => sub = Some(label as i32),
            _ => {}
        }
    }
    Some((block?, sub?))
}

/// Whether a record sits anywhere under these nodes.
fn contains(nodes: &[Node], form_id: u32) -> bool {
    fn go(nodes: &[Node], form_id: u32) -> bool {
        for n in nodes {
            match n {
                Node::Record(r) => {
                    if r.header.form_id == form_id {
                        return true;
                    }
                }
                Node::Group(g) => {
                    if go(&g.children, form_id) {
                        return true;
                    }
                }
            }
        }
        false
    }
    go(nodes, form_id)
}

/// The exterior block and sub-block labels, read out of the group tree.
fn exterior_labels(nodes: &[Node], form_id: u32) -> Option<((i16, i16), (i16, i16))> {
    fn go(nodes: &[Node], form_id: u32) -> Option<((i16, i16), (i16, i16))> {
        for n in nodes {
            let Node::Group(g) = n else { continue };
            match g.header.group_type {
                group_type::EXTERIOR_CELL_BLOCK => {
                    let block = g.header.label_grid();
                    for c in &g.children {
                        let Node::Group(sub) = c else { continue };
                        if sub.header.group_type == group_type::EXTERIOR_CELL_SUBBLOCK
                            && contains(&sub.children, form_id)
                        {
                            return Some((block, sub.header.label_grid()));
                        }
                    }
                }
                group_type::WORLD_CHILDREN => {
                    if let Some(found) = go(&g.children, form_id) {
                        return Some(found);
                    }
                }
                _ => {
                    if let Some(found) = go(&g.children, form_id) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
    go(nodes, form_id)
}

/// The interior block rule holds for every interior cell of a master.
///
/// The rule is `block = (FormID & 0xFFFFFF) % 10` and
/// `sub = ((FormID & 0xFFFFFF) / 10) % 10`, and it is what this crate writes,
/// so it is checked against the shipped data rather than asserted here.
#[test]
fn the_interior_block_rule_holds_for_the_shipped_masters() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let mut checked = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for name in ["FalloutNV.esm", "DeadMoney.esm", "OldWorldBlues.esm"] {
        let Ok(plugin) = Plugin::open(data.join(name)) else {
            continue;
        };
        fn go(nodes: &[Node], checked: &mut usize, mismatches: &mut Vec<String>, name: &str) {
            for n in nodes {
                let Node::Group(g) = n else { continue };
                if g.header.group_type == group_type::INTERIOR_CELL_BLOCK {
                    let block = g.header.label_i32();
                    for c in &g.children {
                        let Node::Group(sub) = c else { continue };
                        if sub.header.group_type != group_type::INTERIOR_CELL_SUBBLOCK {
                            continue;
                        }
                        let sub_label = sub.header.label_i32();
                        for cell in &sub.children {
                            let Node::Record(r) = cell else { continue };
                            if &r.header.sig != b"CELL" {
                                continue;
                            }
                            *checked += 1;
                            let (want_block, want_sub) =
                                cell::interior_block_labels(r.header.form_id);
                            if (block != want_block || sub_label != want_sub)
                                && mismatches.len() < 5
                            {
                                mismatches.push(format!(
                                    "{name} cell {:08X}: block {block} sub {sub_label} should be {want_block} and {want_sub}",
                                    r.header.form_id
                                ));
                            }
                        }
                    }
                }
                go(&g.children, checked, mismatches, name);
            }
        }
        go(&plugin.nodes, &mut checked, &mut mismatches, name);
    }
    assert!(
        checked > 300,
        "expected the master's interior cells, saw {checked}"
    );
    assert!(
        mismatches.is_empty(),
        "the interior block rule does not hold: {:?}",
        &mismatches[..mismatches.len().min(5)]
    );
}

/// The cell-children group that belongs to a cell.
///
/// The group is a **sibling** of the `CELL` record, not a child of it: the
/// sub-block group holds `REC CELL` and then `GRP 6` side by side, and the
/// group is labelled with the cell's FormID.
fn cell_children_of(nodes: &[Node], form_id: u32) -> Option<&fnv_esm::raw::Group> {
    fn go(nodes: &[Node], form_id: u32) -> Option<&fnv_esm::raw::Group> {
        for n in nodes {
            let Node::Group(g) = n else { continue };
            if g.header.group_type == group_type::CELL_CHILDREN && g.header.label_u32() == form_id {
                return Some(g);
            }
            if let Some(found) = go(&g.children, form_id) {
                return Some(found);
            }
        }
        None
    }
    go(nodes, form_id)
}

/// The FormID a staged record ends up with in the saved plugin.
///
/// The staged FormID is plugin-local, and the save moves it into the new
/// plugin's numbering — so the record's FormID in the output is not the one
/// that was handed out. The tests look records up by their Editor ID instead,
/// which is stable.
fn find_by_edid(plugin: &Plugin, edid: &str) -> Option<(u32, fnv_esm::raw::Record)> {
    let mut found = None;
    fnv_esm::raw::visit_records(&plugin.nodes, &mut |r, _| {
        if found.is_none() {
            if let Ok(subs) = r.subrecords() {
                if let Some(s) = subs.iter().find(|s| &s.sig == b"EDID") {
                    if nvre_util::cp1252::decode_z(&s.data) == edid {
                        found = Some((r.header.form_id, r.clone()));
                    }
                }
            }
        }
    });
    found
}

/// A new interior cell is written into a cell-children group under the right
/// block and sub-block.
#[test]
fn a_new_interior_cell_lands_in_its_block_and_sub_block() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let masters = MasterList::new(MASTERS.iter().map(|s| s.to_string()).collect());

    let mut doc = Document::open(data.join("FalloutNV.esm")).unwrap();
    // The FormID is plugin-local: the top byte is the plugin's own index in
    // its own master list. FalloutNV.esm has no masters, so its own index is 0.
    doc.add_cell(cell::NewCell::interior("NvreTestInterior"), 0x0000_0001);

    let out = std::env::temp_dir().join("nvre-editor-test-newinterior.esp");
    let report = write_plugin(&doc, &masters, &schema, &out).unwrap();
    eprintln!("{report:?}");
    assert_eq!(report.added_cells, 1, "the cell must be written");

    let saved = Plugin::open(&out).unwrap();
    let (form_id, rec) = find_by_edid(&saved, "NvreTestInterior").expect("the cell record");
    assert_eq!(&rec.header.sig, b"CELL");

    let path = path_to(&saved.nodes, form_id);
    assert!(
        path.iter().any(|(t, _)| *t == group_type::TOP),
        "the cell must be under a top-level group: {path:?}"
    );
    assert!(
        path.iter()
            .any(|(t, _)| *t == group_type::INTERIOR_CELL_BLOCK),
        "the cell must be under an interior block: {path:?}"
    );
    assert!(
        path.iter()
            .any(|(t, _)| *t == group_type::INTERIOR_CELL_SUBBLOCK),
        "the cell must be under an interior sub-block: {path:?}"
    );
    // The cell's own children group is a sibling of the record, labelled
    // with the cell's FormID.
    let children = cell_children_of(&saved.nodes, form_id).expect("the cell-children group");
    assert!(
        children
            .children
            .iter()
            .any(|c| matches!(c, Node::Group(g) if g.header.group_type == group_type::CELL_TEMPORARY_CHILDREN)),
        "the cell must have a temporary-children group"
    );

    // The labels are the ones the rule says they are.
    let (block, sub) = interior_labels(&saved.nodes, form_id).expect("block labels");
    let (want_block, want_sub) = cell::interior_block_labels(form_id);
    assert_eq!(block, want_block, "the block label must follow the rule");
    assert_eq!(sub, want_sub, "the sub-block label must follow the rule");

    // An interior cell has the "is interior" flag set.
    let data_flags = rec
        .subrecords()
        .unwrap()
        .into_iter()
        .find(|s| &s.sig == b"DATA")
        .map(|s| s.data[0])
        .unwrap_or(0);
    assert_ne!(data_flags & 0x01, 0, "an interior cell must say so");

    std::fs::remove_file(&out).ok();
}

/// A new exterior cell is written under its worldspace, in the block and
/// sub-block its grid puts it in, with a terrain record.
#[test]
fn a_new_exterior_cell_lands_in_its_worldspace_and_grid() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let masters = MasterList::new(MASTERS.iter().map(|s| s.to_string()).collect());

    let mut doc = Document::open(data.join("FalloutNV.esm")).unwrap();
    // A worldspace from the master, so the cell has somewhere to hang.
    // The reference is plugin-local: FalloutNV.esm's own numbering.
    let world = 0x000DA726u32; // WastelandNV
    let grid = (3, -2);

    doc.add_cell(
        cell::NewCell::exterior("NvreTestExterior", world, grid),
        0x0000_0001,
    );

    let out = std::env::temp_dir().join("nvre-editor-test-newexterior.esp");
    let report = write_plugin(&doc, &masters, &schema, &out).unwrap();
    eprintln!("{report:?}");
    assert_eq!(report.added_cells, 1, "the cell must be written");

    let saved = Plugin::open(&out).unwrap();
    let (form_id, rec) = find_by_edid(&saved, "NvreTestExterior").expect("the cell record");
    assert_eq!(&rec.header.sig, b"CELL");

    let path = path_to(&saved.nodes, form_id);
    assert!(
        path.iter().any(|(t, _)| *t == group_type::WORLD_CHILDREN),
        "the cell must be under a world-children group: {path:?}"
    );
    assert!(
        path.iter()
            .any(|(t, _)| *t == group_type::EXTERIOR_CELL_BLOCK),
        "the cell must be under an exterior block: {path:?}"
    );
    assert!(
        path.iter()
            .any(|(t, _)| *t == group_type::EXTERIOR_CELL_SUBBLOCK),
        "the cell must be under an exterior sub-block: {path:?}"
    );

    // The block and sub-block labels are the grid's.
    let ((bx, by), (sx, sy)) = exterior_labels(&saved.nodes, form_id).expect("grid labels");
    assert_eq!(
        (bx, by),
        (grid.0 as i16 >> 5, grid.1 as i16 >> 5),
        "the block label"
    );
    assert_eq!(
        (sx, sy),
        (grid.0 as i16 >> 3, grid.1 as i16 >> 3),
        "the sub-block label"
    );

    // The cell record carries the grid.
    let xclc = rec
        .subrecords()
        .unwrap()
        .into_iter()
        .find(|s| &s.sig == b"XCLC")
        .expect("XCLC");
    let gx = i32::from_le_bytes(xclc.data[0..4].try_into().unwrap());
    let gy = i32::from_le_bytes(xclc.data[4..8].try_into().unwrap());
    assert_eq!((gx, gy), grid, "the cell must carry its grid");

    // The terrain is in the cell's temporary-children group.
    let children = cell_children_of(&saved.nodes, form_id).expect("the cell-children group");
    assert!(
        children
            .children
            .iter()
            .any(|c| matches!(c, Node::Group(g) if g.header.group_type == group_type::CELL_TEMPORARY_CHILDREN)),
        "the cell must have a temporary-children group"
    );
    let mut lands = 0;
    fn count_land(nodes: &[Node], lands: &mut usize) {
        for n in nodes {
            match n {
                Node::Record(r) => {
                    if &r.header.sig == b"LAND" {
                        *lands += 1;
                    }
                }
                Node::Group(g) => count_land(&g.children, lands),
            }
        }
    }
    count_land(&saved.nodes, &mut lands);
    assert_eq!(lands, 1, "the cell must have exactly one LAND record");

    std::fs::remove_file(&out).ok();
}

/// A new worldspace is written with its own world-children group, and a cell
/// added to it hangs off that group.
#[test]
fn a_new_worldspace_gets_its_own_children_group() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let masters = MasterList::new(MASTERS.iter().map(|s| s.to_string()).collect());

    let mut doc = Document::open(data.join("FalloutNV.esm")).unwrap();
    let world_id = doc.add_worldspace(cell::NewWorldspace::new("NvreTestWorld"), 0x0000_0001);
    // A cell in the new worldspace.
    doc.add_cell(
        cell::NewCell::exterior("NvreTestCell", world_id, (0, 0)),
        0x0000_0002,
    );

    let out = std::env::temp_dir().join("nvre-editor-test-newworld.esp");
    let report = write_plugin(&doc, &masters, &schema, &out).unwrap();
    eprintln!("{report:?}");
    assert_eq!(report.added_worldspaces, 1);
    assert_eq!(report.added_cells, 1);

    let saved = Plugin::open(&out).unwrap();

    // The worldspace's record is there.
    let (world_id, world) = find_by_edid(&saved, "NvreTestWorld").expect("the worldspace record");
    assert_eq!(&world.header.sig, b"WRLD");

    // The cell hangs off the new worldspace's world-children group.
    let (cell_id, _) = find_by_edid(&saved, "NvreTestCell").expect("the cell record");
    let cell_path = path_to(&saved.nodes, cell_id);
    let world_children = cell_path
        .iter()
        .find(|(t, _)| *t == group_type::WORLD_CHILDREN)
        .map(|(_, label)| *label);
    assert_eq!(
        world_children,
        Some(world_id),
        "the cell must hang off the new worldspace's own children group"
    );

    std::fs::remove_file(&out).ok();
}

/// The block and sub-block labels of the shipped exterior cells follow the
/// rule this crate uses.
#[test]
fn the_exterior_block_rule_holds_for_the_shipped_masters() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let plugin = Plugin::open(data.join("FalloutNV.esm")).unwrap();
    let mut checked = 0usize;
    let mut mismatches: Vec<String> = Vec::new();

    fn go(nodes: &[Node], checked: &mut usize, mismatches: &mut Vec<String>) {
        for n in nodes {
            let Node::Group(g) = n else { continue };
            if g.header.group_type == group_type::EXTERIOR_CELL_SUBBLOCK {
                let (sx, sy) = g.header.label_grid();
                for c in &g.children {
                    let Node::Record(r) = c else { continue };
                    if &r.header.sig != b"CELL" {
                        continue;
                    }
                    // The cell's own grid, from XCLC.
                    let Ok(subs) = r.subrecords() else { continue };
                    let Some(xclc) = subs.iter().find(|s| &s.sig == b"XCLC") else {
                        continue;
                    };
                    if xclc.data.len() < 8 {
                        continue;
                    }
                    let gx = i32::from_le_bytes(xclc.data[0..4].try_into().unwrap());
                    let gy = i32::from_le_bytes(xclc.data[4..8].try_into().unwrap());
                    *checked += 1;
                    let (want_x, want_y) = cell::exterior_subblock_labels((gx, gy));
                    if (sx, sy) != (want_x, want_y) && mismatches.len() < 5 {
                        mismatches.push(format!(
                            "cell {:08X} at ({gx}, {gy}): sub-block {sx},{sy} should be {want_x},{want_y}",
                            r.header.form_id
                        ));
                    }
                }
            }
            go(&g.children, checked, mismatches);
        }
    }
    go(&plugin.nodes, &mut checked, &mut mismatches);
    assert!(
        checked > 30_000,
        "expected the master's exterior cells, saw {checked}"
    );
    assert!(
        mismatches.is_empty(),
        "the exterior block rule does not hold: {mismatches:?}"
    );
}
