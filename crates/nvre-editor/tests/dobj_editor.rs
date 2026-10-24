//! The Default Object Manager (DOBJ) editor reads and writes every field of
//! every subrecord in fopdoc's DOBJ spec, and keeps the record intact
//! through a save.
//!
//! The spec lists `EDID` and a `DATA` struct of thirty-four FormIDs, one per
//! default object: the stimpacks, the cures, the music, the player voices and
//! the rest. `DOBJ_FIELDS` names them in the order the record stores them.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - One DOBJ record, `DefaultObjectManager` (0x00000031), with a
//!   136-byte `DATA` — thirty-four FormIDs.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// DefaultObjectManager (0x00000031).
const DOBJ_MANAGER: u32 = 0x0000_0031;

#[test]
fn every_field_of_a_real_default_object_manager_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, DOBJ_MANAGER, *b"DOBJ");

    assert_eq!(state.edid, "DefaultObjectManager");

    // --- DATA: the thirty-four default objects ---
    assert_eq!(
        state.dobj_values.len(),
        34,
        "DATA: the record's thirty-four default objects"
    );
    assert_eq!(
        state.dobj_values[0], 0x0001_5169,
        "DATA: the first field (Stimpack)"
    );
    assert_eq!(
        state.dobj_values[1], 0x0008_C551,
        "DATA: the second field (SuperStimpack)"
    );
    assert_eq!(
        state.dobj_values[2], 0x0001_5168,
        "DATA: the third field (RadX)"
    );
    assert_eq!(
        state.dobj_values[6], 0x0001_B2A4,
        "DATA: the seventh field (Player Faction)"
    );
}

#[test]
fn the_field_names_match_the_records_field_count() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, DOBJ_MANAGER, *b"DOBJ");

    assert_eq!(
        nvre_editor::object_editor::DOBJ_FIELDS.len(),
        state.dobj_values.len(),
        "the field-name table covers every field the record has"
    );
}

#[test]
fn a_default_object_manager_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = DOBJ_MANAGER;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"DOBJ");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "dobj_identity");

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
    let fid = DOBJ_MANAGER;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"DOBJ");

    // DATA: the first and the last of the default objects.
    state.dobj_values[0] = 0x0012_3456;
    let last = state.dobj_values.len() - 1;
    state.dobj_values[last] = 0x0065_4321;

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "dobj_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 136, "DATA keeps its 136 bytes");
    assert_eq!(
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        0x0012_3456,
        "the edited first field"
    );
    let at = 136 - 4;
    assert_eq!(
        u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]),
        0x0065_4321,
        "the edited last field"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, DOBJ_MANAGER, *b"DOBJ");
    let title = nvre_editor::object_editor::window_title(
        &s.schema,
        &state.sig,
        &state.edid,
        &state.full_name,
    );
    assert_eq!(title, "Default Object Manager - DefaultObjectManager");
}

#[test]
fn every_default_object_manager_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"DOBJ", 1);
}
