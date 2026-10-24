//! A plugin with **no masters**: the file GECK writes when nothing is ticked.
//!
//! GECK will make a plugin that depends on no master at all. Its output is a
//! TES4 header and nothing else — 56 bytes, no `MAST`/`DATA` pair — and its
//! `next_object_id` starts at `0x800`, not at the `0xADD` a plugin with a master
//! gets. Both values below are measured from files GECK wrote:
//!
//! - `testfiles/fonv_geck_empty_no_master.esp` — 56 bytes, `next_object_id`
//!   `0x00000800`, no masters;
//! - `testfiles/fonv_geck_empty.esp` — 90 bytes, `next_object_id` `0x00000ADD`,
//!   one master (`FalloutNV.esm`).
//!
//! The fixture bytes are embedded rather than read from `testfiles/` because
//! that directory is not part of the repository; the comment above names the
//! file each one came from, and the byte-exact assertion is what makes the
//! comparison meaningful.

use nvre_edit::document::PendingRecord;
use nvre_edit::save::{first_object_id, write_plugin, MasterList};
use nvre_edit::Document;
use nvre_esm::{LoadOrder, Plugin};

/// `testfiles/fonv_geck_empty_no_master.esp`, byte for byte.
///
/// TES4, `data_size` 32, flags 0 (a plugin, not a master), then `HEDR`
/// (version 1.34, 0 records, next object ID `0x800`) and `CNAM` `DEFAULT`.
const GECK_NO_MASTER_ESP: [u8; 56] = [
    0x54, 0x45, 0x53, 0x34, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x48, 0x45, 0x44, 0x52, 0x0c, 0x00, 0x1f, 0x85,
    0xab, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x43, 0x4e, 0x41, 0x4d, 0x08, 0x00,
    0x44, 0x45, 0x46, 0x41, 0x55, 0x4c, 0x54, 0x00,
];

/// The same file with the master flag set: `fonv_geck_empty_no_master.esm`.
fn geck_no_master_esm() -> [u8; 56] {
    let mut b = GECK_NO_MASTER_ESP;
    b[8] = 0x01;
    b
}

/// A blank plugin with no masters is the file GECK writes for the same thing.
///
/// This is the strongest statement available: not "it parses", but "it is
/// byte-identical to what the game's own editor produced".
#[test]
fn a_blank_plugin_with_no_masters_matches_gecks_file() {
    let doc = Document::create_empty("NoMasters.esp", false, &[]).unwrap();
    assert_eq!(
        doc.plugin().nodes.len(),
        1,
        "a no-master plugin is its TES4 header and nothing else"
    );
    let bytes = doc.to_bytes().unwrap();
    assert_eq!(
        bytes, GECK_NO_MASTER_ESP,
        "a no-master .esp must be the 56 bytes GECK writes, not a header with an empty master list"
    );

    let as_master = Document::create_empty("NoMasters.esm", true, &[]).unwrap();
    assert_eq!(
        as_master.to_bytes().unwrap(),
        geck_no_master_esm(),
        "the master flag is the only difference between the .esp and the .esm"
    );
}

/// The header advertises `0x800` for a no-master plugin and `0xADD` when there
/// is a master — both measured from GECK, and neither may drift.
#[test]
fn the_first_object_id_depends_on_whether_a_master_is_declared() {
    assert_eq!(first_object_id(0), 0x0000_0800);
    assert_eq!(first_object_id(1), 0x0000_0ADD);
    assert_eq!(first_object_id(3), 0x0000_0ADD);

    // And the header the editor builds carries the same number.
    let blank = Document::create_empty("NoMasters.esp", false, &[]).unwrap();
    assert_eq!(blank.plugin().header.next_object_id, 0x0000_0800);
    let with_master =
        Document::create_empty("WithMaster.esp", false, &["FalloutNV.esm".to_string()]).unwrap();
    assert_eq!(with_master.plugin().header.next_object_id, 0x0000_0ADD);
}

/// A record placed in a no-master plugin gets an ID above the engine's reserved
/// range, and the header moves past it.
///
/// Offsets below `0x800` are the engine's hardcoded FormIDs: a record written
/// there overrides an engine record instead of being a new one. A no-master
/// plugin's own index is `0`, so an ID handed out from 1 would land in exactly
/// that range.
#[test]
fn a_new_record_in_a_no_master_plugin_is_above_the_reserved_range() {
    let schema = nvre_esm::Schema::default();
    let mut doc = Document::create_empty("NoMasters.esp", false, &[]).unwrap();
    doc.insert(PendingRecord {
        cell: 0x0000_0800,
        record: nvre_edit::document::make_refr(
            0,
            0x0000_0801,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            Some("NoMasterRef"),
            None,
        ),
    });

    let out = std::env::temp_dir().join("nvre-no-masters-ids.esp");
    let report = write_plugin(&doc, &MasterList::new(Vec::new()), &schema, &out).unwrap();
    assert_eq!(report.inserted, 1);

    let saved = Plugin::open(&out).unwrap();
    assert!(
        saved.masters().is_empty(),
        "the saved file must still declare no masters, got {:?}",
        saved.masters()
    );
    assert_eq!(
        saved.header.next_object_id, 0x0000_0801,
        "the header must advertise the ID after the one record it wrote"
    );

    let mut placed = Vec::new();
    nvre_esm::raw::visit_nodes(&saved.nodes, &mut |r, _| {
        if r.header.sig != *b"TES4" {
            placed.push(r.header.form_id);
        }
    });
    assert_eq!(placed.len(), 1, "one record was placed");
    assert_eq!(
        placed[0], 0x0000_0800,
        "a no-master plugin's own index is 0, and its first record starts at 0x800"
    );
    std::fs::remove_file(&out).ok();
}

/// A no-master plugin's worldspace, cell and reference each get a different
/// FormID, and none of them collides.
///
/// A no-master plugin is where this went wrong first: the session hands a new
/// worldspace `0x800` from its own counter, and the save allocated the placed
/// reference from a *separate* counter that also started at `0x800`, so the
/// reference and the worldspace were written with the same FormID. Every
/// plugin was exposed to it; a standalone one, whose two counters start at the
/// same number, collided on the very first record.
#[test]
fn staged_cells_and_worldspaces_do_not_collide_with_placed_records() {
    let schema = nvre_esm::Schema::default();
    let mut doc = Document::create_empty("NoMasters.esp", false, &[]).unwrap();

    // What the session does: take an ID from its own counter for each staged
    // thing, then place a record staged at the placeholder 0.
    let world = doc.add_worldspace(
        nvre_edit::NewWorldspace {
            edid: "NVFStandalone".into(),
            full: None,
            default_land_height: 0.0,
            default_water_height: 0.0,
            climate: None,
            water: None,
        },
        0x0000_0800,
    );
    let cell = doc.add_cell(
        nvre_edit::NewCell {
            edid: "NVFStandaloneCell".into(),
            full: None,
            world: Some(world),
            grid: Some((0, 0)),
            with_land: false,
            data_flags: 0,
        },
        0x0000_0801,
    );
    doc.insert(PendingRecord {
        cell,
        record: nvre_edit::document::make_refr(
            0,
            world,
            [0.0, 0.0, 100.0],
            [0.0, 0.0, 0.0],
            Some("NVFStandaloneRef"),
            None,
        ),
    });

    let out = std::env::temp_dir().join("nvre-no-masters-collide.esp");
    write_plugin(&doc, &MasterList::new(Vec::new()), &schema, &out).unwrap();

    let saved = Plugin::open(&out).unwrap();
    let mut ids: Vec<(String, u32)> = Vec::new();
    nvre_esm::raw::visit_nodes(&saved.nodes, &mut |r, _| {
        if r.header.sig != *b"TES4" {
            ids.push((r.sig_str(), r.header.form_id));
        }
    });
    ids.sort_by_key(|(_, f)| *f);
    assert_eq!(
        ids,
        vec![
            ("WRLD".to_string(), 0x0000_0800),
            ("CELL".to_string(), 0x0000_0801),
            ("REFR".to_string(), 0x0000_0802),
        ],
        "the worldspace, the cell and the reference must each have their own FormID"
    );
    assert_eq!(
        saved.header.next_object_id, 0x0000_0803,
        "the header must advertise the ID after the last one written"
    );
    std::fs::remove_file(&out).ok();
}

/// A no-master plugin loads as a load order of one, and the reader agrees that
/// every record in it is the file's own.
#[test]
fn a_no_master_plugin_is_a_valid_load_order_of_one() {
    let plugin = Plugin::parse(&GECK_NO_MASTER_ESP, "NoMasters.esp".to_string()).unwrap();
    assert_eq!(
        plugin.self_index(),
        0,
        "no masters means its own index is 0"
    );
    assert!(plugin.masters().is_empty());

    let lo = LoadOrder::new(std::slice::from_ref(&plugin)).expect("a no-master plugin loads");
    assert_eq!(lo.plugins.len(), 1);
    assert!(
        lo.index.is_empty(),
        "the header defines no records, so nothing is indexed"
    );
    assert!(
        lo.unmappable.is_empty(),
        "and nothing is unmappable: {:?}",
        lo.unmappable
    );
}

/// A file that declares no masters keeps declaring none when it is re-saved.
///
/// The failure this guards is the plausible one: `rewrite_header` rebuilding the
/// master list from the session's load order and writing the game's master back
/// into a file the user deliberately made standalone.
#[test]
fn saving_a_no_master_plugin_does_not_add_a_master() {
    let schema = nvre_esm::Schema::default();
    let doc = Document::from_plugin(
        Plugin::parse(&GECK_NO_MASTER_ESP, "NoMasters.esp".to_string()).unwrap(),
    );
    let out = std::env::temp_dir().join("nvre-no-masters-resave.esp");
    write_plugin(&doc, &MasterList::new(Vec::new()), &schema, &out).unwrap();

    let saved = Plugin::open(&out).unwrap();
    assert!(
        saved.masters().is_empty(),
        "no master may be added on save, got {:?}",
        saved.masters()
    );
    assert_eq!(
        saved.header.next_object_id, 0x0000_0800,
        "the header keeps the numbering it was created with"
    );
    assert_eq!(
        std::fs::read(&out).unwrap(),
        GECK_NO_MASTER_ESP,
        "re-saving an untouched no-master plugin reproduces GECK's bytes"
    );
    std::fs::remove_file(&out).ok();
}
