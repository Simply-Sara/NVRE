//! The Animation Object (ANIO) editor reads and writes every field of every
//! subrecord in fopdoc's ANIO spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, the model data collection (`MODL`, `MODB`, `MODT`,
//! `MODS`, `MODD`) and `DATA`, a FormID naming the `IDLE` record the
//! animation plays.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 152 ANIO records, every one of them `EDID MODL DATA` in that order.
//! - `aoKhanTrick` (0x001724D1): the model `animobjects\KhanTrick.NIF` and
//!   the animation `0x00026EB4`.
//! - No ANIO record carries `MODB`, `MODS` or `MODD`.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// aoKhanTrick (0x001724D1).
const ANIO_KHAN: u32 = 0x0017_24D1;

/// NVGrindingWheelMachete (0x0016AC98): a different model and animation.
const ANIO_WHEEL: u32 = 0x0016_AC98;

#[test]
fn every_field_of_a_real_animation_object_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ANIO_KHAN, *b"ANIO");

    assert_eq!(state.edid, "aoKhanTrick");

    // --- DATA: the animation ---
    assert_eq!(
        state.anio_animation,
        Some(0x0002_6EB4),
        "DATA: the IDLE record the animation plays"
    );

    // --- The model data collection ---
    let model = &state.anio_model;
    assert_eq!(
        model.modl, "animobjects\\KhanTrick.NIF",
        "MODL: the model path"
    );
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
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ANIO_KHAN, *b"ANIO");
    assert_eq!(state.anio_animation, Some(0x0002_6EB4));

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ANIO_WHEEL, *b"ANIO");
    assert_eq!(state.edid, "NVGrindingWheelMachete");
    assert_eq!(
        state.anio_animation,
        Some(0x0013_395F),
        "DATA: this record's own animation"
    );
    assert_eq!(
        state.anio_model.modl, "animobjects\\NV_GrindWheelMachete.NIF",
        "MODL: this record's own model path"
    );
}

#[test]
fn an_animation_object_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = ANIO_KHAN;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ANIO");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "anio_identity");

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
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = ANIO_KHAN;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ANIO");

    // DATA: the animation.
    state.anio_animation = Some(0x0012_3456);

    // MODL: the model path, through the header's Model Path field.
    state.model = "animobjects\\KhanTrickNew.NIF".to_string();

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "anio_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 4, "DATA is four bytes");
    assert_eq!(
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        0x0012_3456,
        "the edited animation"
    );

    let modl = payload(&after, b"MODL");
    assert_eq!(
        String::from_utf8_lossy(&modl).trim_end_matches('\0'),
        "animobjects\\KhanTrickNew.NIF",
        "the edited model path"
    );

    // The subrecords the record never had are still absent.
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
    state.load_from_session(&s, ANIO_KHAN, *b"ANIO");
    let title = nvre_editor::object_editor::window_title(
        &s.schema,
        &state.sig,
        &state.edid,
        &state.full_name,
    );
    assert_eq!(title, "Animated Object - aoKhanTrick");
}

#[test]
fn every_animation_object_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"ANIO", 152);
}
