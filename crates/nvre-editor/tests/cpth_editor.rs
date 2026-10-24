//! The Camera Path (CPTH) editor reads and writes every field of every
//! subrecord in fopdoc's CPTH spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, the `CTDA` conditions, `ANAM`, a one-byte `DATA`
//! and the `SNAM` camera shots. The conditions and the shots are both
//! *repeated* subrecords, so they are written as lists.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 418 CPTH records, every one carrying a one-byte `DATA`.
//! - `SecLeft` (0x00175C37): two `CTDA` conditions, a parent of
//!   `0x00175C35`, a previous sibling of `0x00175C36`, a `DATA` of 0
//!   (Default), and one `SNAM` shot.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, payloads, saved_subs, sig_order,
    subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// SecLeft (0x00175C37).
const CPTH_LEFT: u32 = 0x0017_5C37;

/// SecRight (0x00175C36): three conditions and a null previous sibling.
const CPTH_RIGHT: u32 = 0x0017_5C36;

#[test]
fn every_field_of_a_real_camera_path_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CPTH_LEFT, *b"CPTH");

    assert_eq!(state.edid, "SecLeft");

    // --- DATA: the camera zoom ---
    assert_eq!(state.cpth_camera_zoom, 0, "DATA: Camera Zoom (Default)");

    // --- ANAM: the parent and the previous sibling ---
    assert_eq!(
        state.cpth_parent,
        Some(0x0017_5C35),
        "ANAM: Parent Camera Path"
    );
    assert_eq!(
        state.cpth_previous,
        Some(0x0017_5C36),
        "ANAM: Previous Sibling Camera Path"
    );

    // --- The CTDA conditions ---
    assert_eq!(
        state.cpth_conditions.len(),
        2,
        "CTDA: the path's conditions"
    );

    // --- The SNAM camera shots ---
    assert_eq!(state.cpth_shots.len(), 1, "SNAM: the path's camera shot");
    assert_eq!(state.cpth_shots[0], 0x0017_5C3A, "SNAM: the camera shot");
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CPTH_LEFT, *b"CPTH");
    assert_eq!(state.cpth_conditions.len(), 2);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CPTH_RIGHT, *b"CPTH");
    assert_eq!(state.edid, "SecRight");
    assert_eq!(
        state.cpth_conditions.len(),
        3,
        "CTDA: this record's own conditions"
    );
    assert_eq!(
        state.cpth_previous,
        Some(0),
        "ANAM: this record's previous sibling is null"
    );
    assert_eq!(
        state.cpth_shots[0], 0x0017_5C39,
        "SNAM: this record's own camera shot"
    );
}

#[test]
fn the_conditions_and_the_shots_survive_a_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The CTDA conditions and the SNAM shots are both *repeated* subrecords.
    // Written with a per-subrecord edit they would collapse onto their last
    // entry.
    let fid = CPTH_LEFT;
    let before = subs(&s, fid);
    assert_eq!(payloads(&before, b"CTDA").len(), 2, "two conditions before");
    assert_eq!(payloads(&before, b"SNAM").len(), 1, "one shot before");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CPTH");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "cpth_lists");

    assert_eq!(
        payloads(&after, b"CTDA"),
        payloads(&before, b"CTDA"),
        "the conditions come back whole and in order"
    );
    assert_eq!(
        payloads(&after, b"SNAM"),
        payloads(&before, b"SNAM"),
        "the shots come back whole and in order"
    );
}

#[test]
fn a_camera_path_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = CPTH_LEFT;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CPTH");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "cpth_identity");

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
    let fid = CPTH_LEFT;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CPTH");

    // DATA: the camera zoom.
    state.cpth_camera_zoom = 2; // Shot List

    // ANAM: the parent and the previous sibling.
    state.cpth_parent = Some(0x0012_3456);
    state.cpth_previous = Some(0x0012_3457);

    // SNAM: the camera shot.
    state.cpth_shots[0] = 0x0012_3458;

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "cpth_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 1, "DATA is one byte");
    assert_eq!(data[0], 2, "the edited camera zoom");

    let anam = payload(&after, b"ANAM");
    assert_eq!(anam.len(), 8, "ANAM is eight bytes");
    assert_eq!(
        u32::from_le_bytes([anam[0], anam[1], anam[2], anam[3]]),
        0x0012_3456,
        "the edited parent camera path"
    );
    assert_eq!(
        u32::from_le_bytes([anam[4], anam[5], anam[6], anam[7]]),
        0x0012_3457,
        "the edited previous sibling"
    );

    let snam = payloads(&after, b"SNAM");
    assert_eq!(snam.len(), 1, "SNAM: the shot is still there");
    assert_eq!(
        u32::from_le_bytes([snam[0][0], snam[0][1], snam[0][2], snam[0][3]]),
        0x0012_3458,
        "the edited camera shot"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CPTH_LEFT, *b"CPTH");
    let title = nvre_editor::object_editor::window_title(
        &s.schema,
        &state.sig,
        &state.edid,
        &state.full_name,
    );
    assert_eq!(title, "Camera Path - SecLeft");
}

#[test]
fn every_camera_path_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"CPTH", 418);
}
