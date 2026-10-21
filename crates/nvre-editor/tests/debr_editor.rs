//! The Debris (DEBR) editor reads and writes every field of every subrecord
//! in fopdoc's DEBR spec, and keeps the record intact through a save.
//!
//! The spec lists `EDID` and the Debris Model collection, where each entry
//! is a `DATA` (the percentage, the model filename and the flags) and a
//! `MODT` (the texture hashes). Both repeat, so the collection is a list of
//! pairs and is written back as one sequence.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 6 DEBR records, carrying between 4 and 6 debris models each.
//! - `RobotGoreBits01` (0x000B8FF4): the first model is `Gore\RoboBit01.NIF`
//!   at 15 per cent, with the "Has Collision Data" flag set and a 72-byte
//!   `MODT` of three texture hashes.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// RobotGoreBits01 (0x000B8FF4).
const DEBR_ROBOT: u32 = 0x000B_8FF4;

#[test]
fn every_field_of_a_real_debris_record_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, DEBR_ROBOT, *b"DEBR");

    assert_eq!(state.edid, "RobotGoreBits01");

    // --- The debris models ---
    assert!(!state.debr_models.is_empty(), "the debris models");
    let first = &state.debr_models[0];
    assert_eq!(first.percentage, 15, "DATA: Percentage");
    assert_eq!(first.model, "Gore\\RoboBit01.NIF", "DATA: Model Filename");
    assert_ne!(first.flags & 0x01, 0, "0x01 Has Collision Data");
    assert_eq!(
        first.texture_hashes.len(),
        72,
        "MODT: 72 bytes of texture hashes"
    );
    assert_eq!(
        first.texture_hashes.len() % 24,
        0,
        "MODT is a whole number of hashes"
    );
}

#[test]
fn a_debris_record_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = DEBR_ROBOT;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"DEBR");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "debr_identity");

    assert_eq!(after.len(), before.len(), "the same subrecords come back");
    for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
        assert_eq!(
            (b.sig, &b.data),
            (a.sig, &a.data),
            "subrecord {i} ({}) is byte-identical",
            nvre_util::tag_str(&a.sig)
        );
    }
    assert_eq!(
        sig_order(&after),
        sig_order(&before),
        "the subrecords come back in the order they were in"
    );
}

#[test]
fn the_debris_models_keep_their_order_through_a_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The collection is a run of `DATA` and `MODT` per model. A per-signature
    // list edit would cluster every `DATA` together and every `MODT`
    // together, which the engine cannot read back into models.
    let fid = DEBR_ROBOT;
    let before = subs(&s, fid);
    let before_order: Vec<String> = sig_order(&before);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"DEBR");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "debr_order");

    assert_eq!(
        sig_order(&after),
        before_order,
        "the DATA and MODT pairs come back interleaved in the same order"
    );
}

#[test]
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = DEBR_ROBOT;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"DEBR");

    // The first model's percentage, filename and flag.
    state.debr_models[0].percentage = 42;
    state.debr_models[0].model = "Gore\\RoboBitNew.NIF".to_string();
    state.debr_models[0].flags &= !0x01; // clear Has Collision Data

    // The texture hashes are opaque file hashes, so the editor keeps them.
    let modt_before = state.debr_models[0].texture_hashes.clone();

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "debr_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data[0], 42, "DATA: the edited percentage");
    assert_eq!(
        String::from_utf8_lossy(&data[1..data.len() - 1]).trim_end_matches('\0'),
        "Gore\\RoboBitNew.NIF",
        "DATA: the edited model filename"
    );
    assert_eq!(
        data[data.len() - 1] & 0x01,
        0,
        "DATA: the edited Has Collision Data flag"
    );

    let modt = payload(&after, b"MODT");
    assert_eq!(
        modt, modt_before,
        "MODT: the texture hashes are copied through, not rewritten"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, DEBR_ROBOT, *b"DEBR");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Debris - RobotGoreBits01");
}

#[test]
fn every_debris_record_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"DEBR", 6);
}
