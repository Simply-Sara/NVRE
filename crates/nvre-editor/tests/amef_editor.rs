//! The Ammo Effect (AMEF) editor reads and writes every field of every
//! subrecord in fopdoc's AMEF spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, `FULL` and a twelve-byte `DATA` holding a type
//! enum, an operation enum and a float32 value.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 54 AMEF records, every one of them `EDID FULL DATA` in that order, with
//!   a twelve-byte `DATA`.
//! - `AmmoEffect5mmHPDTPenalty` (0x0017BA47): type 2 (DT Mod), operation 1
//!   (Multiply), value 2.0.
//! - `AmmoEffectBuckshotDamageBonus` (0x0017BA44): type 0 (Damage Mod),
//!   operation 1 (Multiply), value 1.2.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// AmmoEffect5mmHPDTPenalty (0x0017BA47): DT Mod, Multiply, 2.0.
const AMEF_HP_DT: u32 = 0x0017_BA47;

/// AmmoEffectBuckshotDamageBonus (0x0017BA44): Damage Mod, Multiply, 1.2.
const AMEF_DAMAGE: u32 = 0x0017_BA44;

#[test]
fn every_field_of_a_real_ammo_effect_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, AMEF_HP_DT, *b"AMEF");

    assert_eq!(state.edid, "AmmoEffect5mmHPDTPenalty");
    assert_eq!(state.full_name, "Dam. Thresh. Increase");

    // --- DATA: the type, the operation and the value ---
    assert_eq!(state.amef_type, 2, "DATA: Type (DT Mod)");
    assert_eq!(state.amef_operation, 1, "DATA: Operation (Multiply)");
    assert_eq!(state.amef_value, 2.0, "DATA: Value");
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // A field set only inside a conditional keeps the previous record's value
    // unless it is reset, so the loader is checked against a second record.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, AMEF_HP_DT, *b"AMEF");
    assert_eq!(state.amef_type, 2);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, AMEF_DAMAGE, *b"AMEF");
    assert_eq!(state.edid, "AmmoEffectBuckshotDamageBonus");
    assert_eq!(state.amef_type, 0, "DATA: this record's own type");
    assert_eq!(state.amef_operation, 1, "DATA: this record's own operation");
    assert_eq!(state.amef_value, 1.2, "DATA: this record's own value");
}

#[test]
fn an_ammo_effect_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = AMEF_HP_DT;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"AMEF");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "amef_identity");

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
    let fid = AMEF_HP_DT;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"AMEF");

    // DATA: the type, the operation and the value.
    state.amef_type = 5;
    state.amef_operation = 2;
    state.amef_value = -3.5;

    // FULL: the name.
    state.full_name = "Edited Name".to_string();

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "amef_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 12, "DATA is twelve bytes");
    assert_eq!(
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        5,
        "the edited type"
    );
    assert_eq!(
        u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
        2,
        "the edited operation"
    );
    assert_eq!(
        f32::from_le_bytes([data[8], data[9], data[10], data[11]]),
        -3.5,
        "the edited value"
    );

    let full = payload(&after, b"FULL");
    assert_eq!(
        String::from_utf8_lossy(&full).trim_end_matches('\0'),
        "Edited Name",
        "the edited name"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, AMEF_HP_DT, *b"AMEF");
    let title = nvre_editor::object_editor::window_title(
        &s.schema,
        &state.sig,
        &state.edid,
        &state.full_name,
    );
    assert_eq!(
        title,
        "Ammo Effect - AmmoEffect5mmHPDTPenalty (Dam. Thresh. Increase)"
    );
}

#[test]
fn every_ammo_effect_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"AMEF", 54);
}
