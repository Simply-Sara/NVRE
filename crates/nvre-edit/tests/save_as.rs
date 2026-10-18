//! Saving as a new plugin: the header, the FormID remap and the allocation.
//!
//! These tests answer one question — **is the output loadable as a mod?** A copy
//! of a master is not: a FormID is file-local, so the copied references point at
//! nothing. The checks here use NVForever's own `fnvtool` on a file this crate
//! actually produced, so the verdict comes from an independent reader.

use fnv_esm::{LoadOrder, Plugin, Schema};
use nvre_edit::save::{formid_map_for, write_plugin, write_plugin_scoped, MasterList, Scope};
use nvre_edit::Document;

/// The game's Data folder, when the environment names one.
fn data_dir() -> Option<std::path::PathBuf> {
    std::env::var("NVRE_DATA")
        .ok()
        .map(std::path::PathBuf::from)
}

fn schema_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")
}

/// The masters the tests build a plugin on top of.
const MASTERS: [&str; 2] = ["FalloutNV.esm", "DeadMoney.esm"];

/// Parse the given plugins into a load order, as the editor opens them.
fn load(plugins: &[Plugin]) -> LoadOrder<'_> {
    LoadOrder::new(plugins).expect("load order")
}

/// A plugin saved from a master must carry the master in its own header.
#[test]
fn saved_plugin_names_its_masters() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let plugins: Vec<Plugin> = MASTERS
        .iter()
        .map(|n| Plugin::open(data.join(n)).unwrap())
        .collect();
    let lo = load(&plugins);
    let masters = MasterList::new(MASTERS.iter().map(|s| s.to_string()).collect());

    let doc = Document::open(data.join("FalloutNV.esm")).unwrap();
    let out = std::env::temp_dir().join("nvre-editor-test-masters.esp");
    let report = write_plugin(&doc, &masters, &schema, &out).unwrap();

    eprintln!("{report:?}");
    // The saved file must parse and list both masters.
    let saved = Plugin::open(&out).unwrap();
    let names: Vec<&str> = saved
        .header
        .masters
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, MASTERS, "the new plugin must depend on both masters");
    // It must not be a master file itself.
    assert!(
        !saved.header.is_master(),
        "the output is a .esp, not an .esm"
    );
    // Its own FormIDs must start above the masters' range.
    assert_eq!(
        saved.header.next_object_id, 0x0200_0000,
        "2 masters => own IDs from 0x02xxxxxx"
    );
    let _ = lo;
    std::fs::remove_file(&out).ok();
}

/// A reference to a master's object must be rewritten into the new numbering.
#[test]
fn formids_are_remapped_into_the_new_plugin() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let masters = MasterList::new(MASTERS.iter().map(|s| s.to_string()).collect());

    let mut doc = Document::open(data.join("FalloutNV.esm")).unwrap();

    // FalloutNV.esm has no masters, so its own records are index 0 in its own
    // numbering; in the new plugin they become index 2 (after the two masters).
    let map = formid_map_for(&doc, &masters).unwrap();
    assert_eq!(
        map.get(&0),
        Some(&2),
        "the source's own index 0 becomes the new file's own index 2"
    );

    // A record that carries a FormID: place it, then check the saved bytes. The
    // base object 0x00157C89 is one of FalloutNV.esm's own records, so it must
    // be remapped from index 0 to index 2.
    let schema = Schema::load_dir(schema_dir()).unwrap();
    doc.insert(nvre_edit::document::PendingRecord {
        cell: 0x0031e2a,
        record: nvre_edit::document::make_refr_flags(
            0,
            0x0015_7C89,
            [100.0, 200.0, 300.0],
            [0.0, 0.0, 0.0],
            Some("NVFRemapTest"),
            None,
            nvre_edit::document::REFR_PERSISTENT,
        ),
    });
    let out = std::env::temp_dir().join("nvre-editor-test-remap.esp");
    write_plugin(&doc, &masters, &schema, &out).unwrap();

    let saved = Plugin::open(&out).unwrap();
    let rec = nvre_edit::document::record_of(&saved, 0x0200_0000).unwrap();
    let base = nvre_edit::document::value_of(&schema, rec, b"NAME").unwrap();
    assert_eq!(
        base.as_str(),
        Some("0x02157C89"),
        "the base object must be remapped into the new plugin's numbering"
    );
    std::fs::remove_file(&out).ok();
}

/// An inserted record must get a fresh FormID and land in the right cell.
#[test]
fn inserted_records_get_fresh_formids() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let masters = MasterList::new(MASTERS.iter().map(|s| s.to_string()).collect());

    let mut doc = Document::open(data.join("FalloutNV.esm")).unwrap();

    // Two inserts in one session must not collide.
    for i in 0..2 {
        doc.insert(nvre_edit::document::PendingRecord {
            cell: 0x0031e2a,
            record: nvre_edit::document::make_refr_flags(
                0x00FF_0000 + i,
                0x0015_7C89,
                [100.0 * (i as f32 + 1.0), 200.0, 300.0],
                [0.0, 0.0, 0.0],
                Some("NVFSaveTest"),
                None,
                nvre_edit::document::REFR_PERSISTENT,
            ),
        });
    }
    let out = std::env::temp_dir().join("nvre-editor-test-insert.esp");
    let report = write_plugin(&doc, &masters, &schema, &out).unwrap();
    eprintln!("{report:?}");
    assert_eq!(report.inserted, 2);

    let saved = Plugin::open(&out).unwrap();
    let mut found = Vec::new();
    fnv_esm::raw::visit_records(&saved.nodes[1..], &mut |r, _| {
        if r.header.sig == *b"REFR"
            && nvre_edit::document::edid_of(r).as_deref() == Some("NVFSaveTest")
        {
            found.push(r.header.form_id);
        }
    });
    assert_eq!(found.len(), 2, "both inserted records must be in the file");
    // Both in the new plugin's own FormID space, and distinct.
    for f in &found {
        assert_eq!(f >> 24, 2, "inserted records must be the new plugin's own");
    }
    assert_ne!(found[0], found[1], "two inserts must not share a FormID");

    // And each must sit inside the cell's temporary-children group.
    let mut in_temp = 0;
    fnv_esm::raw::visit_groups(&saved.nodes, &mut |g, _| {
        if g.header.group_type == fnv_esm::raw::group_type::CELL_CHILDREN
            && g.header.label_u32() == 0x0031e2a
        {
            for c in &g.children {
                if let fnv_esm::raw::Node::Group(t) = c {
                    if t.header.group_type == fnv_esm::raw::group_type::CELL_TEMPORARY_CHILDREN {
                        for n in &t.children {
                            if let fnv_esm::raw::Node::Record(r) = n {
                                if r.header.sig == *b"REFR"
                                    && nvre_edit::document::edid_of(r).as_deref()
                                        == Some("NVFSaveTest")
                                {
                                    in_temp += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    assert_eq!(in_temp, 2, "both must be in the cell's temporary children");
    std::fs::remove_file(&out).ok();
}

/// A save must report what it wrote and what it could not remap.
#[test]
fn save_reports_unremapped_subrecords() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let masters = MasterList::new(MASTERS.iter().map(|s| s.to_string()).collect());
    let mut doc = Document::open(data.join("FalloutNV.esm")).unwrap();
    doc.insert(nvre_edit::document::PendingRecord {
        cell: 0x0031e2a,
        record: nvre_edit::document::make_refr_flags(
            0,
            0x0015_7C89,
            [1.0, 2.0, 3.0],
            [0.0, 0.0, 0.0],
            Some("NVFReportTest"),
            None,
            nvre_edit::document::REFR_PERSISTENT,
        ),
    });
    let out = std::env::temp_dir().join("nvre-editor-test-report.esp");
    let report = write_plugin(&doc, &masters, &schema, &out).unwrap();

    // The report must be complete and honest: exactly the session's own records.
    eprintln!(
        "records {} inserted {} remapped_subs {} remapped_formids {} unremapped {}",
        report.records,
        report.inserted,
        report.remapped_subs,
        report.remapped_formids,
        report.unremapped.len()
    );
    assert_eq!(
        report.records, 1,
        "only the session's own record is written"
    );
    assert_eq!(report.inserted, 1);
    // The mod must be small: it does not carry the master's records.
    assert!(
        report.bytes < 4_000_000,
        "a mod with one record must be small, was {}",
        report.bytes
    );
    for p in &report.unremapped {
        eprintln!("  unremapped: {p}");
    }
    // Whatever could not be remapped is named, and the file still parses.
    let saved = Plugin::open(&out).unwrap();
    assert_eq!(saved.header.masters.len(), MASTERS.len());
    std::fs::remove_file(&out).ok();
}

/// A whole-plugin save is the round-trip path, and it says so in its report.
#[test]
fn whole_plugin_scope_copies_every_record() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let masters = MasterList::new(MASTERS.iter().map(|s| s.to_string()).collect());
    let doc = Document::open(data.join("FalloutNV.esm")).unwrap();
    let out = std::env::temp_dir().join("nvre-editor-test-whole.esp");
    let report = write_plugin_scoped(&doc, &masters, &schema, &out, Scope::WholePlugin).unwrap();
    eprintln!(
        "whole-plugin: {} records, {} bytes",
        report.records, report.bytes
    );
    assert!(report.records > 400_000, "every record is written");
    let saved = Plugin::open(&out).unwrap();
    assert_eq!(saved.header.masters.len(), MASTERS.len());
    std::fs::remove_file(&out).ok();
}

/// A master missing from the new list is an error, not a dangling reference.
#[test]
fn missing_master_is_an_error() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let doc = Document::open(data.join("DeadMoney.esm")).unwrap();
    // DeadMoney.esm needs FalloutNV.esm; leave it out.
    let masters = MasterList::new(vec!["DeadMoney.esm".to_string()]);
    let err = formid_map_for(&doc, &masters);
    assert!(
        err.is_err(),
        "a missing master must be reported, not written as a dangling reference"
    );
    if let Err(e) = err {
        eprintln!("error as expected: {e}");
    }
}

/// Editing a master's record must produce an override, not a dropped edit.
#[test]
fn editing_a_master_record_overrides_it() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    // The mod depends on the master it overrides.
    let masters = MasterList::new(vec!["FalloutNV.esm".to_string()]);

    let mut doc = Document::open(data.join("FalloutNV.esm")).unwrap();
    let target = doc
        .records()
        .into_iter()
        .find(|(_, r)| {
            r.header.sig == *b"REFR"
                && r.subrecords()
                    .is_ok_and(|s| s.iter().any(|x| x.sig == *b"DATA"))
        })
        .map(|(f, _)| f)
        .expect("a REFR with DATA");
    let before =
        nvre_edit::document::value_of(&schema, doc.record(target).unwrap(), b"DATA").unwrap();
    let mut new_value = before.clone();
    new_value["position"]["x"] = serde_json::json!(4242.0f64);
    let payload = nvre_edit::encode::encode_sub(&schema, b"REFR", b"DATA", &new_value).unwrap();
    doc.set_sub(target, b"DATA", payload);

    let out = std::env::temp_dir().join("nvre-editor-test-override.esp");
    let report = write_plugin(&doc, &masters, &schema, &out).unwrap();
    eprintln!("override report: {report:?}");

    let saved = Plugin::open(&out).unwrap();
    // The override must be present, under the overridden record's own FormID.
    let rec = nvre_edit::document::record_of(&saved, target)
        .unwrap_or_else(|| panic!("the override for {target:08X} must be in the mod"));
    let after = nvre_edit::document::value_of(&schema, rec, b"DATA").unwrap();
    assert_eq!(
        after["position"]["x"],
        serde_json::json!(4242.0f64),
        "the edit must be applied"
    );
    // The override must be declared in ONAM.
    assert!(
        saved.header.overridden_forms.contains(&target),
        "ONAM must declare {target:08X}, got {:?}",
        saved.header.overridden_forms
    );
    // The mod is still small: it carries the override, not the master.
    assert!(
        report.bytes < 4_000_000,
        "an override mod must stay small, was {}",
        report.bytes
    );
    std::fs::remove_file(&out).ok();
}

/// Deleting a master's reference must override it with the Deleted flag.
#[test]
fn deleting_a_master_reference_overrides_it() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let masters = MasterList::new(vec!["FalloutNV.esm".to_string()]);

    let mut doc = Document::open(data.join("FalloutNV.esm")).unwrap();
    let target = doc
        .records()
        .into_iter()
        .find(|(_, r)| r.header.sig == *b"REFR")
        .map(|(f, _)| f)
        .expect("a REFR");
    let original_flags = doc.record(target).unwrap().header.flags;
    doc.delete_record(target);

    let out = std::env::temp_dir().join("nvre-editor-test-delete.esp");
    let report = write_plugin(&doc, &masters, &schema, &out).unwrap();
    eprintln!("delete report: {report:?}");

    let saved = Plugin::open(&out).unwrap();
    let rec = nvre_edit::document::record_of(&saved, target).unwrap();
    assert_eq!(
        rec.header.flags,
        original_flags | fnv_esm::raw::record_flags::DELETED,
        "the override must carry the Deleted flag"
    );
    assert!(
        saved.header.overridden_forms.contains(&target),
        "ONAM must declare the deletion"
    );
    std::fs::remove_file(&out).ok();
}

/// Overriding a record from a plugin that is not in the master list is an error.
#[test]
fn override_without_the_master_is_an_error() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let masters = MasterList::new(vec!["FalloutNV.esm".to_string()]);

    let mut doc = Document::open(data.join("DeadMoney.esm")).unwrap();
    let target = doc.records().first().map(|(f, _)| *f).unwrap();
    doc.delete_record(target);
    let out = std::env::temp_dir().join("nvre-editor-test-badoverride.esp");
    let err = write_plugin(&doc, &masters, &schema, &out);
    assert!(
        err.is_err(),
        "overriding a plugin that is not a master must be an error"
    );
    if let Err(e) = err {
        eprintln!("error as expected: {e}");
    }
}

/// The override must land in the cell the record actually belongs to.
///
/// `fnv_world::WorldIndex` is the authority on which cell a reference is in; the
/// saved override has to sit in that cell's children group or the load order will
/// not associate it with the record it replaces, and the edit silently does
/// nothing in-game while every structural check still passes.
#[test]
fn override_lands_in_the_records_own_cell() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let masters = MasterList::new(vec!["FalloutNV.esm".to_string()]);

    let mut doc = Document::open(data.join("FalloutNV.esm")).unwrap();
    let target = doc
        .records()
        .into_iter()
        .find(|(_, r)| {
            r.header.sig == *b"REFR"
                && r.subrecords()
                    .is_ok_and(|s| s.iter().any(|x| x.sig == *b"DATA"))
        })
        .map(|(f, _)| f)
        .expect("a REFR with DATA");

    // Which cell the load order says the record is in.
    let expected_cell = {
        let lo = LoadOrder::new(std::slice::from_ref(doc.plugin())).unwrap();
        let ix = fnv_world::WorldIndex::build(&lo, &Schema::default());
        ix.refs
            .get(&target)
            .map(|e| e.cell)
            .unwrap_or_else(|| panic!("{target:08X} must be in the world index"))
    };

    let before =
        nvre_edit::document::value_of(&schema, doc.record(target).unwrap(), b"DATA").unwrap();
    let mut new_value = before.clone();
    new_value["position"]["x"] = serde_json::json!(4242.0f64);
    let payload = nvre_edit::encode::encode_sub(&schema, b"REFR", b"DATA", &new_value).unwrap();
    doc.set_sub(target, b"DATA", payload);

    let out = std::env::temp_dir().join("nvre-editor-test-cell.esp");
    write_plugin(&doc, &masters, &schema, &out).unwrap();

    let saved = Plugin::open(&out).unwrap();
    // Find the cell-children group holding the override, and check its label.
    let mut found_cell = None;
    fnv_esm::raw::visit_groups(&saved.nodes, &mut |g, _| {
        if g.header.group_type != fnv_esm::raw::group_type::CELL_CHILDREN {
            return;
        }
        let holds = g.children.iter().any(|c| match c {
            fnv_esm::raw::Node::Record(r) => r.header.form_id == target,
            // A record may also sit in the cell's persistent/temporary groups.
            fnv_esm::raw::Node::Group(inner) => inner.children.iter().any(|n| match n {
                fnv_esm::raw::Node::Record(r) => r.header.form_id == target,
                _ => false,
            }),
        });
        if holds {
            found_cell = Some(g.header.label_u32());
        }
    });
    assert_eq!(
        found_cell,
        Some(expected_cell),
        "the override must sit in cell {expected_cell:08X}, the cell the load order says it belongs to"
    );
    std::fs::remove_file(&out).ok();
}
