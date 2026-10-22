//! The property grid edits any field of any subrecord, and writes back only
//! what changed.
//!
//! A field edit is not a value written into a struct: the subrecord is a byte
//! layout, so editing one field means decoding the whole subrecord, replacing
//! the field, and re-encoding. What has to hold is that the bytes the editor did
//! not touch come back identical, and that a value the layout rejects changes
//! nothing at all. Both are checked against a real reference in the game's own
//! master, written out and read back through the same reader the editor uses.

use nvre_editor::session::Session;

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

fn open_master() -> Option<Session> {
    let data = data_dir()?;
    Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}

/// Open the cell with this Editor ID.
fn open(s: &mut Session, edid: &str) {
    let cell = s
        .index
        .cells
        .values()
        .find(|c| c.edid.as_deref() == Some(edid))
        .map(|c| c.form_id)
        .unwrap_or_else(|| panic!("{edid}"));
    s.open_cell(cell);
}

/// A subrecord's decoded value on a record.
fn view(s: &Session, fid: u32, sig: &str) -> serde_json::Value {
    let rec = s.doc.record(fid).expect("the record");
    nvre_edit::encode::sub_views(&s.schema, rec)
        .expect("subrecord views")
        .into_iter()
        .find(|v| v.sig == sig)
        .map(|v| v.value)
        .unwrap_or(serde_json::Value::Null)
}

/// A scratch file for a saved plugin, unique per test.
fn out(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("nvre_prop_{name}.esp"))
}

/// Write the session out and read the record back through a fresh parse.
fn saved_view(s: &Session, fid: u32, sig: &str, name: &str) -> serde_json::Value {
    let path = out(name);
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = nvre_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), format!("{name}.esp"))
        .expect("parse");
    let mut found = None;
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        if r.header.form_id == fid {
            found = Some(r.clone());
        }
    });
    let rec = found.expect("the record survives the save");
    let v = nvre_edit::encode::sub_views(&s.schema, &rec)
        .expect("subrecord views")
        .into_iter()
        .find(|v| v.sig == sig)
        .map(|v| v.value)
        .unwrap_or(serde_json::Value::Null);
    let _ = std::fs::remove_file(&path);
    v
}

/// A subrecord signature as the `[u8; 4]` the schema and the document want.
fn sig4(s: &str) -> [u8; 4] {
    let b = s.as_bytes();
    [
        b.first().copied().unwrap_or(0),
        b.get(1).copied().unwrap_or(0),
        b.get(2).copied().unwrap_or(0),
        b.get(3).copied().unwrap_or(0),
    ]
}

#[test]
fn a_field_edit_writes_back_and_leaves_the_rest_of_the_subrecord_alone() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // VendorChestsCell: nine references, six of them locked, so a record with an
    // XLOC to edit is easy to find.
    open(&mut s, "VendorChestsCell");
    let refs = s.cell_refs().to_vec();
    let target = refs
        .iter()
        .find(|r| r.lock.is_some())
        .expect("a locked reference");
    let fid = target.form_id;
    s.select(fid);

    let before = view(&s, fid, "XLOC");
    let level_before = before
        .get("level")
        .and_then(|v| v.as_u64())
        .expect("a level");
    // A level the game reports: 25 is Easy, one of the six GECK offers.
    assert_ne!(
        level_before, 25,
        "pick a record whose level is not already 25"
    );
    s.set_selected_field(b"XLOC", "level", serde_json::json!(25))
        .expect("a level the layout accepts");

    // The queued edit is not applied to the record in the document, so the
    // record still reads its original bytes — the edit lives in the queue until
    // the save, which is the same behaviour every other edit in this editor has.
    assert_eq!(
        view(&s, fid, "XLOC").get("level").and_then(|v| v.as_u64()),
        Some(level_before),
        "the record itself is untouched until the save"
    );
    // What the save writes is the new value.
    let after = saved_view(&s, fid, "XLOC", "field_edit");
    assert_eq!(
        after.get("level").and_then(|v| v.as_u64()),
        Some(25),
        "the level survives the save"
    );

    // Every other field of the subrecord is byte-identical. XLOC is 20 bytes:
    // level, three unused, key, flags, three more unused. An edit that
    // re-encoded from scratch rather than from the decoded value would have to
    // reproduce the unused bytes exactly, and they are not in the JSON the
    // editor edits.
    assert_eq!(after.get("unused"), before.get("unused"), "unused bytes");
    assert_eq!(after.get("key"), before.get("key"), "the key");
    assert_eq!(after.get("flags"), before.get("flags"), "the flags");
    assert_eq!(
        after.get("unused2"),
        before.get("unused2"),
        "unused bytes 2"
    );
}

#[test]
fn a_bare_scalar_subrecord_is_editable_as_the_whole_value() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    open(&mut s, "VendorChestsCell");
    let refs = s.cell_refs().to_vec();
    // A reference with an owner: XOWN is a bare 4-byte formid with no inner
    // fields, so it is addressed by the empty path.
    let target = refs
        .iter()
        .find(|r| r.owner.is_some())
        .expect("an owned reference");
    let fid = target.form_id;
    s.select(fid);
    let before = view(&s, fid, "XOWN");
    assert!(before.is_string(), "XOWN decodes to a formid string");

    s.set_selected_field(b"XOWN", "", serde_json::json!("0x00000001"))
        .expect("an owner the layout accepts");
    let after = saved_view(&s, fid, "XOWN", "scalar");
    assert_eq!(after, serde_json::json!("0x00000001"), "the new owner");
    // And a field path into a scalar has nothing to name, so it is refused
    // rather than silently ignored.
    assert!(
        s.set_selected_field(b"XOWN", "level", serde_json::json!(1))
            .is_err(),
        "a scalar has no fields"
    );
}

#[test]
fn a_value_the_layout_rejects_changes_nothing() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    open(&mut s, "VendorChestsCell");
    let refs = s.cell_refs().to_vec();
    let target = refs
        .iter()
        .find(|r| r.lock.is_some())
        .expect("a locked reference");
    let fid = target.form_id;
    s.select(fid);
    let before = view(&s, fid, "XLOC");
    let pending_before = s.pending_edits();

    // A u8 field cannot hold 300. The encoder refuses it, so nothing is queued
    // and the field keeps its value — a silent truncation would write a level
    // the modder never typed.
    let err = s
        .set_selected_field(b"XLOC", "level", serde_json::json!(300))
        .expect_err("300 does not fit a u8");
    assert!(
        err.contains("out of range"),
        "the error says what was wrong: {err}"
    );
    assert_eq!(view(&s, fid, "XLOC"), before, "the field is untouched");
    assert_eq!(
        s.pending_edits(),
        pending_before,
        "nothing was queued for a refused edit"
    );

    // A field the layout does not name is refused too: writing it would add a
    // key the encoder rejects, so the panel says so instead of appearing to
    // accept the edit.
    assert!(s
        .set_selected_field(b"XLOC", "nope", serde_json::json!(1))
        .is_err());
    assert_eq!(view(&s, fid, "XLOC"), before, "still untouched");
    assert_eq!(s.pending_edits(), pending_before);

    // And a subrecord the record does not carry is refused: appending one is a
    // different operation, and the panel is not offering it here.
    let err = s
        .set_selected_field(b"XSCL", "", serde_json::json!(1.0))
        .expect_err("this reference has no XSCL");
    assert!(
        err.contains("no XSCL"),
        "the error names the subrecord: {err}"
    );
    assert_eq!(s.pending_edits(), pending_before);
}

#[test]
fn every_field_of_a_placed_reference_is_reachable() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    open(&mut s, "VendorChestsCell");
    let refs = s.cell_refs().to_vec();
    let target = refs
        .iter()
        .find(|r| r.owner.is_some())
        .expect("an owned ref");
    let fid = target.form_id;
    s.select(fid);
    let rec = s.doc.record(fid).expect("the record");
    let views = nvre_edit::encode::sub_views(&s.schema, rec).expect("views");

    // The point of the panel: every subrecord lists its fields, including a bare
    // scalar's own value. A subrecord that lists nothing has a field the modder
    // cannot reach, which is what the old raw-JSON panel was.
    for v in &views {
        let fields = nvre_edit::encode::field_names(&s.schema, &rec.header.sig, &sig4(&v.sig));
        assert!(
            !fields.is_empty(),
            "{} ({}) lists no fields, so none of it is editable",
            v.name,
            v.sig
        );
    }
    // And each of those paths either resolves inside the decoded value, or names
    // an optional field this record does not carry — which the panel skips,
    // because there is nothing to show for a field that is not there.
    for v in &views {
        let fields = nvre_edit::encode::field_names(&s.schema, &rec.header.sig, &sig4(&v.sig));
        for path in fields {
            let mut at = &v.value;
            for part in path.split('.') {
                if part.is_empty() {
                    continue;
                }
                at = at
                    .as_object()
                    .and_then(|o| o.get(part))
                    .unwrap_or(&serde_json::Value::Null);
            }
            // XLOC's trailing 8 bytes are optional in the schema, so a 20-byte
            // XLOC has no `unknown` to show. That is a field the record does not
            // carry, not one the modder cannot reach.
            assert!(
                !at.is_null() || v.sig == "XLOC" && path == "unknown",
                "{}.{path} has no value to edit",
                v.sig
            );
        }
    }
}

#[test]
fn a_nested_structs_fields_are_addressable_paths() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    open(&mut s, "VendorChestsCell");
    let refs = s.cell_refs().to_vec();
    let fid = refs[0].form_id;
    s.select(fid);
    let rec = s.doc.record(fid).expect("the record");

    // DATA is two structs deep: position and rotation, each an xyz. Its fields
    // must be addressable paths, because `get_path` indexes the decoded value by
    // them. A flattened "x" names nothing — a reference's position and rotation
    // are the most-edited fields in the editor, so this is the case that has to
    // hold.
    let fields = nvre_edit::encode::field_names(&s.schema, &rec.header.sig, b"DATA");
    for want in [
        "position",
        "position.x",
        "position.y",
        "position.z",
        "rotation_radians",
        "rotation_radians.x",
        "rotation_radians.y",
        "rotation_radians.z",
    ] {
        assert!(
            fields.iter().any(|f| f == want),
            "DATA lists {fields:?}, which does not address {want}"
        );
    }
    // And each of those paths resolves inside the decoded value, so the widget
    // has a current value to show and an edit can be queued.
    let data = view(&s, fid, "DATA");
    for path in &fields {
        let mut at = &data;
        for part in path.split('.') {
            at = at.as_object().and_then(|o| o.get(part)).expect(path);
        }
    }

    // Editing a field two structs down writes back, and leaves its siblings
    // alone: the position and the rotation share one subrecord, so moving x must
    // not touch y or the rotation.
    let before = view(&s, fid, "DATA");
    let x_before = before["position"]["x"].as_f64().expect("position.x");
    s.set_selected_field(b"DATA", "position.x", serde_json::json!(1234.5))
        .expect("a position the layout accepts");
    let after = saved_view(&s, fid, "DATA", "nested");
    assert_eq!(after["position"]["x"].as_f64(), Some(1234.5), "x moved");
    assert_eq!(
        after["position"]["y"], before["position"]["y"],
        "y is untouched"
    );
    assert_eq!(
        after["rotation_radians"], before["rotation_radians"],
        "the rotation is untouched"
    );
    assert!((x_before - 1234.5).abs() > 1.0, "the move is a real move");
}
