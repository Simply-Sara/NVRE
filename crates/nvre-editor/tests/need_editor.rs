//! The need records — `DEHY`, `HUNG`, `RADS` and `SLPD` — share one editor.
//!
//! All four have the same shape: `EDID` and an eight-byte `DATA` holding a
//! trigger threshold and the actor effect the trigger applies. The editor
//! gives each of them its own page, and each page is checked against the
//! game's own values.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - Five DEHY, five HUNG, five RADS and five SLPD records, every one of
//!   them `EDID DATA` with an eight-byte `DATA`.
//! - `Water5` (0x00161905): a threshold of 1000 and the effect `0x0015FA20`.
//! - `Water4` (0x00161904): a threshold of 800 and the effect `0x0015FA1F`.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// Water5 (0x00161905).
const DEHY_WATER5: u32 = 0x0016_1905;

/// Water4 (0x00161904).
const DEHY_WATER4: u32 = 0x0016_1904;

/// hunger5 (0x0016190A).
const HUNG_5: u32 = 0x0016_190A;

#[test]
fn every_field_of_a_real_dehydration_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, DEHY_WATER5, *b"DEHY");

    assert_eq!(state.edid, "Water5");
    assert_eq!(
        state.dehyd_trigger_threshold, 1000,
        "DATA: Trigger Threshold"
    );
    assert_eq!(
        state.dehyd_actor_effect,
        Some(0x0015_FA20),
        "DATA: Actor Effect"
    );
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, DEHY_WATER5, *b"DEHY");
    assert_eq!(state.dehyd_trigger_threshold, 1000);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, DEHY_WATER4, *b"DEHY");
    assert_eq!(state.edid, "Water4");
    assert_eq!(
        state.dehyd_trigger_threshold, 800,
        "DATA: this record's own threshold"
    );
    assert_eq!(
        state.dehyd_actor_effect,
        Some(0x0015_FA1F),
        "DATA: this record's own actor effect"
    );
}

#[test]
fn the_hunger_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HUNG_5, *b"HUNG");

    assert_eq!(state.edid, "hunger5");
    assert_eq!(
        state.hung_trigger_threshold, 1000,
        "DATA: Trigger Threshold"
    );
    assert_eq!(
        state.hung_actor_effect,
        Some(0x0015_FA25),
        "DATA: Actor Effect"
    );
}

#[test]
fn a_dehydration_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = DEHY_WATER5;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"DEHY");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "dehy_identity");

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
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = DEHY_WATER5;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"DEHY");

    // DATA: the threshold and the actor effect.
    state.dehyd_trigger_threshold = 2500;
    state.dehyd_actor_effect = Some(0x0012_3456);

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "dehy_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 8, "DATA is eight bytes");
    assert_eq!(
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        2500,
        "the edited trigger threshold"
    );
    assert_eq!(
        u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
        0x0012_3456,
        "the edited actor effect"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, DEHY_WATER5, *b"DEHY");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Dehydration - Water5");
}

#[test]
fn every_dehydration_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"DEHY", 5);
}

#[test]
fn every_hunger_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"HUNG", 5);
}

#[test]
fn every_radiation_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"RADS", 5);
}

#[test]
fn every_sleep_deprivation_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"SLPD", 5);
}
