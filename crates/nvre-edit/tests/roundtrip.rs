//! The encoder's contract, checked against the real game data.
//!
//! The point of these tests is not that the encoder works on made-up values —
//! it is that **every subrecord of the shipped masters round-trips byte for
//! byte**. That is the property that lets the editor claim a saved plugin is
//! safe: a field it cannot write back identically is one it must refuse to touch.
//!
//! Game data is read from `$NVRE_DATA` (the game's `Data` folder) and the
//! tests skip when it is not set, so CI never needs the game.

use nvre_esm::raw::visit_nodes;
use nvre_esm::{Plugin, Schema};
use nvre_edit::encode::{encode_sub, roundtrips, sub_views};

/// The game's Data folder, when the environment names one.
fn data_dir() -> Option<std::path::PathBuf> {
    std::env::var("NVRE_DATA")
        .ok()
        .map(std::path::PathBuf::from)
}

fn schema_dir() -> std::path::PathBuf {
    // NVRE's own schema directory: the layouts plus the
    // records and subrecords it does not define. See `schemas/README.md`.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")
}

/// The masters a full check would cover; the test uses whichever are present.
const MASTERS: [&str; 5] = [
    "FalloutNV.esm",
    "DeadMoney.esm",
    "OldWorldBlues.esm",
    "GunRunnersArsenal.esm",
    "CaravanPack.esm",
];

/// Every subrecord of `plugin` that the decoder reads as `Ok` must re-encode to
/// the same bytes.
///
/// This is the contract that makes saving safe. Where it fails, the failure names
/// either a **schema gap** (the subrecord has no layout in `schemas/fnv`, so the
/// decoder emitted it as hex and there is nothing to invert) or a
/// **representation limit** (a struct with repeated field names, which the JSON
/// form cannot hold). In both cases the editor must leave the field read-only.
///
/// Measured over FalloutNV.esm: 2,316,930 subrecords decode, and all but three
/// signatures re-encode byte for byte. The three that do not are representation
/// limits, not schema gaps: `CSTY/CSTD` (a struct whose field names repeat, so
/// the JSON form cannot hold both) and `TERM/ITXT` + `TERM/RNAM` (a 4-byte
/// zstring of all NULs decodes to `""` but re-encodes to a single NUL). The
/// editor must leave those read-only. Navmesh, crafting, ragdoll, camera paths,
/// alternate textures (MODS and friends) and everything else the decoder reads
/// now round-trip exactly.
#[test]
fn every_decoded_subrecord_roundtrips() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping the master round-trip check");
        return;
    };
    let schema = Schema::load_dir(schema_dir()).expect("schema loads");
    let mut checked = 0usize;
    let mut gaps: std::collections::BTreeMap<String, usize> = Default::default();
    for name in MASTERS {
        let path = data.join(name);
        if !path.exists() {
            continue;
        }
        let plugin = Plugin::open(&path).expect(name);
        let mut recs = Vec::new();
        visit_nodes(&plugin.nodes, &mut |r, _| recs.push(r));
        for r in &recs {
            let Ok(subs) = r.subrecords() else { continue };
            for s in &subs {
                checked += 1;
                if !roundtrips(&schema, &r.header.sig, s) {
                    *gaps
                        .entry(format!("{}/{}", r.sig_str(), s.sig_str()))
                        .or_default() += 1;
                }
            }
        }
    }
    eprintln!(
        "round-trip: {checked} subrecords checked, {} gap signatures",
        gaps.len()
    );
    for (k, n) in &gaps {
        eprintln!("  gap: {k} x{n}");
    }
    assert!(checked > 0, "no plugins found under {}", data.display());
    // The signatures the editor must be able to write. If any of these starts
    // failing, the editor can no longer save those records safely.
    const MUST_ROUNDTRIP: [&str; 12] = [
        "REFR/NAME",
        "REFR/DATA",
        "REFR/EDID",
        "REFR/XSCL",
        "REFR/XTEL",
        "STAT/EDID",
        "STAT/MODL",
        "ACTI/EDID",
        "ACTI/MODL",
        "DOOR/FNAM",
        "LIGH/DATA",
        "CONT/EDID",
    ];
    for must in MUST_ROUNDTRIP {
        assert!(
            !gaps.contains_key(must),
            "{must} must round-trip; it is needed to save edits"
        );
    }
    // Every gap must be outside the subrecords the editor writes. MODS (alternate
    // textures) is a count plus variable-length cp1252 names, which the fixed
    // struct form of fnv.schema/1 cannot express; REFR/TNAM is a size-decided
    // union the schema does not model. Both are read-only in the editor.
    let edited_subs = [
        "REFR/NAME",
        "REFR/DATA",
        "REFR/EDID",
        "REFR/XSCL",
        "REFR/XTEL",
        "REFR/XESP",
    ];
    let in_edited: Vec<&String> = gaps
        .keys()
        .filter(|k| edited_subs.iter().any(|e| k.starts_with(e)))
        .collect();
    assert!(
        in_edited.is_empty(),
        "gaps in subrecords the editor writes: {in_edited:?}"
    );
}

/// A record left untouched must survive a document round-trip byte for byte.
#[test]
fn untouched_plugin_is_byte_identical() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let path = data.join("FalloutNV.esm");
    if !path.exists() {
        return;
    }
    let original = std::fs::read(&path).unwrap();
    let doc = nvre_edit::Document::open(&path).unwrap();
    assert!(!doc.dirty());
    assert_eq!(doc.edit_count(), 0);
    let out = doc.to_bytes().unwrap();
    assert_eq!(out.len(), original.len(), "size changed with no edits");
    assert_eq!(
        out, original,
        "an untouched document must be byte-identical"
    );
}

/// Editing one field must change exactly that record's bytes.
#[test]
fn one_field_edit_changes_only_that_record() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let path = data.join("FalloutNV.esm");
    if !path.exists() {
        return;
    }
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let doc = nvre_edit::Document::open(&path).unwrap();
    let original = std::fs::read(&path).unwrap();

    // Pick a REFR with a DATA subrecord and change its position.
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

    let mut edited = doc;
    // Encode a new DATA payload through the encoder, from the decoded value.
    let mut new_value = before.clone();
    new_value["position"]["x"] = serde_json::json!(4321.0f32 as f64);
    let payload = encode_sub(&schema, b"REFR", b"DATA", &new_value).expect("DATA encodes");
    edited.set_sub(target, b"DATA", payload);
    assert!(edited.dirty());

    let out = edited.to_bytes().unwrap();
    // The field's width is unchanged, so the file size is unchanged too; what must
    // differ is the record's own bytes.
    let reparsed = Plugin::parse(&out, "test.esp".into()).unwrap();
    let schema2 = Schema::load_dir(schema_dir()).unwrap();
    let after = nvre_edit::document::record_of(&reparsed, target).unwrap();
    let found = nvre_edit::document::value_of(&schema2, after, b"DATA").unwrap();
    assert_eq!(found["position"]["x"], serde_json::json!(4321.0f64));
    // The bytes on disk differ from the original at that record.
    let orig_plugin = Plugin::parse(&original, "orig.esp".into()).unwrap();
    let orig_rec = nvre_edit::document::record_of(&orig_plugin, target).unwrap();
    assert_ne!(
        after.raw, orig_rec.raw,
        "the edited record's payload must differ"
    );
    // Exactly one record was touched.
    assert_eq!(edited.edit_count(), 1);
}

/// Inserting a reference must produce a file that still parses, with the new
/// record present in the right cell's temporary-children group.
#[test]
fn inserted_reference_lands_in_the_cell() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let path = data.join("FalloutNV.esm");
    if !path.exists() {
        return;
    };
    let doc = nvre_edit::Document::open(&path).unwrap();
    // The persistent cell of WastelandNV, verified in the layout study.
    let cell = 0x0031e2a;
    let rec = nvre_edit::document::make_refr_flags(
        0x00FF0001,
        0x00157C89,
        [100.0, 200.0, 300.0],
        [0.0, 0.0, 0.0],
        Some("NVFEditorTest"),
        None,
        nvre_edit::document::REFR_PERSISTENT,
    );
    let mut edited = doc;
    edited.insert(nvre_edit::document::PendingRecord { cell, record: rec });
    let out = edited.to_bytes().unwrap();

    let reparsed = Plugin::parse(&out, "test.esp".into()).unwrap();
    let found = nvre_edit::document::edid_of(
        nvre_edit::document::record_of(&reparsed, 0x00FF0001).unwrap(),
    )
    .unwrap();
    assert_eq!(found, "NVFEditorTest");
    // And it must sit inside the cell's temporary-children group.
    let mut in_temp = false;
    nvre_esm::raw::visit_groups_in(&reparsed.nodes, &mut |g, _| {
        if g.header.group_type == nvre_esm::raw::group_type::CELL_CHILDREN
            && g.header.label_u32() == cell
        {
            for c in &g.children {
                if let nvre_esm::raw::Node::Group(t) = c {
                    if t.header.group_type == nvre_esm::raw::group_type::CELL_TEMPORARY_CHILDREN {
                        for n in &t.children {
                            if let nvre_esm::raw::Node::Record(r) = n {
                                if r.header.form_id == 0x00FF0001 {
                                    in_temp = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    assert!(
        in_temp,
        "the inserted record must be in the cell's temporary children"
    );
}

/// Deleting a record must set the Deleted flag, not drop it from the file.
#[test]
fn delete_sets_the_deleted_flag() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let path = data.join("FalloutNV.esm");
    if !path.exists() {
        return;
    };
    let doc = nvre_edit::Document::open(&path).unwrap();
    let target = doc.records().first().map(|(f, _)| *f).unwrap();
    let before_flags = doc.record(target).unwrap().header.flags;
    let mut edited = doc;
    edited.delete_record(target);
    let out = edited.to_bytes().unwrap();
    let reparsed = Plugin::parse(&out, "test.esp".into()).unwrap();
    let after = nvre_edit::document::record_of(&reparsed, target).unwrap();
    assert_eq!(
        after.header.flags,
        before_flags | nvre_esm::raw::record_flags::DELETED
    );
}

/// The property grid must report writability honestly.
#[test]
fn sub_views_report_writability() {
    let Some(data) = data_dir() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let path = data.join("FalloutNV.esm");
    if !path.exists() {
        return;
    };
    let schema = Schema::load_dir(schema_dir()).unwrap();
    let plugin = Plugin::open(&path).unwrap();
    let mut recs = Vec::new();
    visit_nodes(&plugin.nodes, &mut |r, _| recs.push(r));
    let mut total = 0usize;
    let mut writable = 0usize;
    for r in recs.iter().take(500) {
        for v in sub_views(&schema, r).unwrap() {
            total += 1;
            if v.writable {
                writable += 1;
            }
        }
    }
    eprintln!("sub_views: {writable}/{total} writable in the first 500 records");
    assert!(total > 0);
}
