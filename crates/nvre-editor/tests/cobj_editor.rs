//! The Constructible Object (COBJ) record's coverage.
//!
//! The shipped masters hold no `COBJ` records, so this suite builds one
//! from the spec's own layout and proves the page reads and writes every
//! field it names.

mod common;

use common::{rawsub, write_synthetic_plugin, zsub};
use nvre_editor::object_editor::ObjectEditorState;

/// A `COBJ` record with every field the spec names, at a form id in the
/// plugin's own range.
fn build_cobj(path: &std::path::Path) -> u32 {
    let form_id = 0x0F000001;

    // `DATA` — the value and the weight.
    let mut data = Vec::new();
    data.extend_from_slice(&(-42i32).to_le_bytes());
    data.extend_from_slice(&3.5f32.to_le_bytes());

    // `OBND` — the object's min and max corners.
    let mut obnd = Vec::new();
    for v in [-10i16, -20, -30] {
        obnd.extend_from_slice(&v.to_le_bytes());
    }
    for v in [10i16, 20, 30] {
        obnd.extend_from_slice(&v.to_le_bytes());
    }

    let subs = vec![
        zsub(b"EDID", "TestConstructible"),
        rawsub(b"OBND", obnd),
        zsub(b"FULL", "Test Constructible Object"),
        zsub(b"MODL", "Meshes\\Clutter\\ClutterBottles\\EmptyBottle.NIF"),
        zsub(b"ICON", "IconConstructible"),
        zsub(b"MICO", "MicoConstructible"),
        rawsub(b"SCRI", 0x0F000010u32.to_le_bytes().to_vec()),
        rawsub(b"YNAM", 0x0F000011u32.to_le_bytes().to_vec()),
        rawsub(b"ZNAM", 0x0F000012u32.to_le_bytes().to_vec()),
        rawsub(b"DATA", data),
    ];

    write_synthetic_plugin(path, *b"COBJ", form_id, subs);
    form_id
}

fn open(path: &std::path::Path) -> nvre_editor::session::Session {
    nvre_editor::session::Session::open(
        path,
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec![],
    )
    .expect("open the synthetic plugin")
}

#[test]
fn the_constructible_object_reads_every_named_field() {
    let path = std::env::temp_dir().join("nvre_cobj_read.esp");
    let form_id = build_cobj(&path);
    let s = open(&path);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, form_id, *b"COBJ");

    assert_eq!(state.cobj_value, -42, "the value");
    assert_eq!(state.cobj_weight, 3.5, "the weight");
    assert_eq!(state.cobj_name, "Test Constructible Object", "the name");
    assert_eq!(state.cobj_icon, "IconConstructible", "the large icon");
    assert_eq!(state.cobj_mico, "MicoConstructible", "the small icon");
    assert_eq!(state.cobj_script, Some(0x0F000010), "the script");
    assert_eq!(
        state.cobj_sound_pickup,
        Some(0x0F000011),
        "the pick up sound"
    );
    assert_eq!(state.cobj_sound_drop, Some(0x0F000012), "the drop sound");
    assert_eq!(state.cobj_bounds_min, [-10, -20, -30], "the bounds' min");
    assert_eq!(state.cobj_bounds_max, [10, 20, 30], "the bounds' max");
    assert_eq!(
        state.cobj_model.modl, "Meshes\\Clutter\\ClutterBottles\\EmptyBottle.NIF",
        "the model path"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_constructible_object_writes_every_named_field() {
    let path = std::env::temp_dir().join("nvre_cobj_write.esp");
    let form_id = build_cobj(&path);
    let mut s = open(&path);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, form_id, *b"COBJ");

    state.cobj_value = 99;
    state.cobj_weight = 12.25;
    state.cobj_name = "Renamed Object".to_string();
    state.cobj_icon = "IconRenamed".to_string();
    state.cobj_mico = "MicoRenamed".to_string();
    state.cobj_script = Some(0x0F000020);
    state.cobj_sound_pickup = Some(0x0F000021);
    state.cobj_sound_drop = Some(0x0F000022);
    state.cobj_bounds_min = [-1, -2, -3];
    state.cobj_bounds_max = [4, 5, 6];
    state.cobj_model.modl = "Meshes\\New\\New.nif".to_string();
    state.save_to_session(&mut s);

    // Read the record back out of the file the editor wrote.
    let out = std::env::temp_dir().join("nvre_cobj_write_out.esp");
    s.save(&out).expect("save");
    let bytes = std::fs::read(&out).expect("read back");
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "out.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&out);

    let mut found = None;
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
        if r.header.form_id == form_id {
            found = Some(r.clone());
        }
    });
    let record = found.expect("the record survives the save");
    let subs = record.subrecords().expect("subrecords parse");

    let data = common::payload(&subs, b"DATA");
    assert_eq!(
        i32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        99,
        "the value is written"
    );
    assert_eq!(
        f32::from_le_bytes([data[4], data[5], data[6], data[7]]),
        12.25,
        "the weight is written"
    );

    let obnd = common::payload(&subs, b"OBND");
    assert_eq!(
        i16::from_le_bytes([obnd[0], obnd[1]]),
        -1,
        "the bounds' min x is written"
    );
    assert_eq!(
        i16::from_le_bytes([obnd[10], obnd[11]]),
        6,
        "the bounds' max z is written"
    );

    assert_eq!(
        String::from_utf8_lossy(&common::payload(&subs, b"FULL"))
            .trim_end_matches('\0')
            .to_string(),
        "Renamed Object",
        "the name is written"
    );
    assert_eq!(
        String::from_utf8_lossy(&common::payload(&subs, b"ICON"))
            .trim_end_matches('\0')
            .to_string(),
        "IconRenamed",
        "the large icon is written"
    );
    assert_eq!(
        String::from_utf8_lossy(&common::payload(&subs, b"MICO"))
            .trim_end_matches('\0')
            .to_string(),
        "MicoRenamed",
        "the small icon is written"
    );

    let script = common::payload(&subs, b"SCRI");
    assert_eq!(
        u32::from_le_bytes([script[0], script[1], script[2], script[3]]),
        0x0F000020,
        "the script is written"
    );
    let pickup = common::payload(&subs, b"YNAM");
    assert_eq!(
        u32::from_le_bytes([pickup[0], pickup[1], pickup[2], pickup[3]]),
        0x0F000021,
        "the pick up sound is written"
    );
    let drop = common::payload(&subs, b"ZNAM");
    assert_eq!(
        u32::from_le_bytes([drop[0], drop[1], drop[2], drop[3]]),
        0x0F000022,
        "the drop sound is written"
    );
    assert_eq!(
        String::from_utf8_lossy(&common::payload(&subs, b"MODL"))
            .trim_end_matches('\0')
            .to_string(),
        "Meshes\\New\\New.nif",
        "the model path is written"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_constructible_object_saves_back_unchanged() {
    let path = std::env::temp_dir().join("nvre_cobj_same.esp");
    let form_id = build_cobj(&path);
    let mut s = open(&path);

    let before = common::subs(&s, form_id)
        .iter()
        .map(|x| (x.sig, x.data.clone()))
        .collect::<Vec<_>>();

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, form_id, *b"COBJ");
    state.save_to_session(&mut s);

    let out = std::env::temp_dir().join("nvre_cobj_same_out.esp");
    s.save(&out).expect("save");
    let bytes = std::fs::read(&out).expect("read back");
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "out.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&out);

    let mut after = None;
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
        if r.header.form_id == form_id {
            after = Some(
                r.subrecords()
                    .expect("subs")
                    .iter()
                    .map(|x| (x.sig, x.data.clone()))
                    .collect::<Vec<_>>(),
            );
        }
    });
    let after = after.expect("the record survives the save");

    assert_eq!(
        after.len(),
        before.len(),
        "the same number of subrecords comes back"
    );
    for (i, (bsig, bdata)) in before.iter().enumerate() {
        assert_eq!(
            &after[i],
            &(*bsig, bdata.clone()),
            "subrecord {i} ({}) is byte-identical",
            nvre_util::tag_str(bsig)
        );
    }

    let _ = std::fs::remove_file(&path);
}
