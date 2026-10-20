//! The Addon Node (ADDN) editor reads and writes every field of every
//! subrecord in fopdoc's ADDN spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, `OBND`, the model data collection (`MODL`, `MODB`,
//! `MODT`, `MODS`, `MODD`), `DATA` (a node index) and `DNAM` (a master
//! particle system cap plus two unnamed bytes). What is checked here is that
//! the values are the game's own values, and that a save reproduces the
//! record byte for byte when nothing was changed.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with
//! `fnvtool esm survey` and a raw subrecord dump, not against a fixture
//! written from memory. The measured facts the tests lean on:
//!
//! - 37 ADDN records, every one of them `EDID OBND MODL MODT DATA DNAM` in
//!   that order, with a four-byte `DATA` and a four-byte `DNAM`.
//! - `DATA` is a small int32 node index: the records measured carry 33, 34,
//!   35 and 36.
//! - `DNAM` is `0000 0100` on every record measured: a zero cap and the two
//!   unnamed bytes `01 00`.
//! - No ADDN record carries `MODB`, `MODS` or `MODD`.

use nvre_editor::object_editor::ObjectEditorState;
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

/// The record's subrecords, in file order.
fn subs(s: &Session, fid: u32) -> Vec<fnv_esm::Subrecord> {
    let rec = s.doc.record(fid).expect("the record");
    rec.subrecords().expect("subrecords parse")
}

/// The same, read from the file the editor actually wrote.
fn saved_subs(s: &Session, fid: u32, name: &str) -> Vec<fnv_esm::Subrecord> {
    let path = std::env::temp_dir().join(format!("nvre_addn_{name}.esp"));
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), format!("{name}.esp"))
        .expect("parse");
    let mut found = None;
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
        if r.header.form_id == fid {
            found = Some(r.clone());
        }
    });
    let _ = std::fs::remove_file(&path);
    found
        .expect("the record survives the save")
        .subrecords()
        .expect("subrecords parse")
}

/// A subrecord's payload by signature.
fn payload(subs: &[fnv_esm::Subrecord], sig: &[u8; 4]) -> Vec<u8> {
    subs.iter()
        .find(|s| &s.sig == sig)
        .map(|s| s.data.clone())
        .unwrap_or_else(|| panic!("a {} subrecord", fnv_util::tag_str(sig)))
}

/// The signature sequence of a record, in file order.
fn sig_order(subs: &[fnv_esm::Subrecord]) -> Vec<String> {
    subs.iter()
        .map(|s| fnv_util::tag_str(&s.sig).to_string())
        .collect()
}

/// MPSFlamerFlame01 (0x000BEDF5): node index 36, a 48-byte `MODT` with two
/// texture hashes, and bounds that are the game's "no bounds" sentinel.
const ADDN_FLAMER: u32 = 0x000B_EDF5;

/// MPSSmokeSmall01 (0x000BE960): node index 35, a 24-byte `MODT` with one
/// texture hash, and real bounds.
const ADDN_SMOKE: u32 = 0x000B_E960;

#[test]
fn every_field_of_a_real_addon_node_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ADDN_FLAMER, *b"ADDN");

    assert_eq!(state.edid, "MPSFlamerFlame01");

    // --- DATA: the node index ---
    assert_eq!(state.addn_node_index, 36, "DATA: Node Index");

    // --- DNAM: the master particle system cap and the unnamed bytes ---
    assert_eq!(state.addn_master_cap, 0, "DNAM: Master Particle System Cap");
    assert_eq!(
        state.addn_dnam_unknown,
        [0x01, 0x00],
        "DNAM: the two bytes the spec does not name"
    );

    // --- The model data collection ---
    let model = &state.addn_model;
    assert_eq!(
        model.modl, "MPS\\MPSFlamerFlame01.NIF",
        "MODL: the model's path"
    );
    assert_eq!(model.modt.len(), 48, "MODT: 48 bytes of texture hashes");
    assert_eq!(model.modt.len() % 24, 0, "MODT is a whole number of hashes");
    assert_eq!(model.modb, None, "MODB: this record has none");
    assert_eq!(model.mods.len(), 0, "MODS: this record has none");
    assert_eq!(model.modd, None, "MODD: this record has none");

    // The header's Model Path field is the collection's MODL.
    assert_eq!(state.model, model.modl, "the header shows the same path");
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The point of the second fixture: the loader must read this record's own
    // values, not the previous record's. A field set only inside a conditional
    // keeps the previous record's value unless it is reset.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ADDN_FLAMER, *b"ADDN");
    assert_eq!(state.addn_node_index, 36);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ADDN_SMOKE, *b"ADDN");
    assert_eq!(state.edid, "MPSSmokeSmall01");
    assert_eq!(
        state.addn_node_index, 35,
        "DATA: this record's own node index"
    );
    assert_eq!(
        state.addn_model.modl, "MPS\\MPSSmokeSmall01.NIF",
        "MODL: this record's own model path"
    );
    assert_eq!(
        state.addn_model.modt.len(),
        24,
        "MODT: this record's own hash count"
    );
}

#[test]
fn an_addon_node_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = ADDN_FLAMER;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ADDN");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "identity");

    assert_eq!(after.len(), before.len(), "the same subrecords come back");
    for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
        assert_eq!(
            (b.sig, &b.data),
            (a.sig, &a.data),
            "subrecord {i} ({}) is byte-identical",
            fnv_util::tag_str(&a.sig)
        );
    }
    assert_eq!(
        sig_order(&after),
        sig_order(&before),
        "the subrecords come back in the order they were in"
    );
}

#[test]
fn the_texture_hashes_are_carried_through_a_no_edit_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // MODT holds 24-byte texture hashes. A texture hash is a file hash the
    // editor cannot compute, so it must be copied through exactly.
    let fid = ADDN_SMOKE;
    let before = payload(&subs(&s, fid), b"MODT");
    assert_eq!(before.len(), 24, "the record's MODT is 24 bytes");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ADDN");
    state.addn_node_index = 99; // edit something else entirely
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "modt"), b"MODT");
    assert_eq!(after, before, "the texture hashes are byte-identical");
}

#[test]
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = ADDN_FLAMER;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ADDN");

    // DATA: the node index.
    state.addn_node_index = 1234;

    // DNAM: the master particle system cap.
    state.addn_master_cap = 42;

    // MODL: the model's path, through the header's Model Path field.
    state.model = "MPS\\MPSFlamerFlame01New.NIF".to_string();

    // MODT: the texture hashes are opaque file hashes, so the editor keeps
    // them; the test checks they survive an edit of another field.
    let modt_before = state.addn_model.modt.clone();

    // MODB and MODD: neither is in this record, so the editor must not
    // invent them.
    assert_eq!(state.addn_model.modb, None, "no MODB to begin with");
    assert_eq!(state.addn_model.modd, None, "no MODD to begin with");

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "edited");

    // DATA.
    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 4, "DATA is four bytes");
    assert_eq!(
        i32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        1234,
        "the edited node index"
    );

    // DNAM.
    let dnam = payload(&after, b"DNAM");
    assert_eq!(dnam.len(), 4, "DNAM is four bytes");
    assert_eq!(
        u16::from_le_bytes([dnam[0], dnam[1]]),
        42,
        "the edited master particle system cap"
    );
    assert_eq!(
        [dnam[2], dnam[3]],
        [0x01, 0x00],
        "the unnamed bytes are carried through"
    );

    // MODL.
    let modl = payload(&after, b"MODL");
    assert_eq!(
        String::from_utf8_lossy(&modl).trim_end_matches('\0'),
        "MPS\\MPSFlamerFlame01New.NIF",
        "the edited model path"
    );

    // MODT: the hashes are preserved byte for byte.
    assert_eq!(
        payload(&after, b"MODT"),
        modt_before,
        "the texture hashes are copied through, not rewritten"
    );

    // And the subrecords the record never had are still absent.
    assert!(
        !after.iter().any(|s| s.sig == *b"MODB"),
        "no MODB is invented"
    );
    assert!(
        !after.iter().any(|s| s.sig == *b"MODD"),
        "no MODD is invented"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ADDN_FLAMER, *b"ADDN");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Addon Node - MPSFlamerFlame01");
}

#[test]
fn every_addon_node_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The strongest statement: every one of the game's 37 ADDN records,
    // loaded and saved with no edit, comes back byte for byte.
    let fids: Vec<u32> = s
        .doc
        .records()
        .into_iter()
        .filter(|(_, r)| r.header.sig == *b"ADDN")
        .map(|(f, _)| f)
        .collect();
    assert_eq!(fids.len(), 37, "the master has 37 ADDN records");

    let mut before = std::collections::BTreeMap::new();
    for fid in &fids {
        let subs = subs(&s, *fid);
        before.insert(
            *fid,
            subs.iter()
                .map(|x| (x.sig, x.data.clone()))
                .collect::<Vec<_>>(),
        );
    }

    for fid in &fids {
        let mut state = ObjectEditorState::default();
        state.load_from_session(&s, *fid, *b"ADDN");
        state.save_to_session(&mut s);
    }

    let path = std::env::temp_dir().join("nvre_addn_all.esp");
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "all.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&path);

    let mut after = std::collections::BTreeMap::new();
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
        if r.header.sig == *b"ADDN" {
            after.insert(
                r.header.form_id,
                r.subrecords()
                    .expect("subs")
                    .iter()
                    .map(|x| (x.sig, x.data.clone()))
                    .collect::<Vec<_>>(),
            );
        }
    });

    for fid in &fids {
        let b = before.get(fid).expect("before");
        let a = after.get(fid).expect("after");
        assert_eq!(
            a.len(),
            b.len(),
            "{fid:08X}: the same number of subrecords comes back"
        );
        for (i, (bs, bd)) in b.iter().enumerate() {
            let (asig, ad) = &a[i];
            assert_eq!(
                (asig, ad),
                (bs, bd),
                "{fid:08X}: subrecord {i} ({}) is byte-identical",
                fnv_util::tag_str(bs)
            );
        }
    }
}
