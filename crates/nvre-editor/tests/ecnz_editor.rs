//! The Encounter Zone (ECZN) editor reads and writes every field of every
//! subrecord in fopdoc's ECZN spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID` and an eight-byte `DATA`: the owner, a rank, a
//! minimum level, a flag word and one byte the spec does not name.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 17 ECZN records, every one carrying an eight-byte `DATA`.
//! - `HooverDamZone` (0x00131F33): a null owner, a rank of 0, a minimum
//!   level of 10 and no flags.
//! - `GomorrahZone` (0x0010CC90): a rank of -1 and a minimum level of 22.
//! - `SSHQZone` (0x0010654A): the "Never Resets" flag.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// HooverDamZone (0x00131F33).
const ECZN_HOOVER: u32 = 0x0013_1F33;

/// GomorrahZone (0x0010CC90): a negative rank.
const ECZN_GOMORRAH: u32 = 0x0010_CC90;

/// SSHQZone (0x0010654A): the "Never Resets" flag.
const ECZN_SSHQ: u32 = 0x0010_654A;

#[test]
fn every_field_of_a_real_encounter_zone_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ECZN_HOOVER, *b"ECZN");

    assert_eq!(state.edid, "HooverDamZone");
    assert_eq!(state.ecnz_owner, Some(0), "DATA: Owner is null");
    assert_eq!(state.ecnz_rank, 0, "DATA: Rank");
    assert_eq!(state.ecnz_minimum_level, 10, "DATA: Minimum Level");
    assert_eq!(state.ecnz_flags, 0, "DATA: Flags");
    assert_eq!(
        state.ecnz_unused, 0,
        "DATA: the byte the spec does not name"
    );
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ECZN_HOOVER, *b"ECZN");
    assert_eq!(state.ecnz_minimum_level, 10);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ECZN_GOMORRAH, *b"ECZN");
    assert_eq!(state.edid, "GomorrahZone");
    assert_eq!(state.ecnz_rank, -1, "DATA: this record's own negative rank");
    assert_eq!(
        state.ecnz_minimum_level, 22,
        "DATA: this record's own minimum level"
    );
}

#[test]
fn the_never_resets_flag_is_read() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ECZN_SSHQ, *b"ECZN");

    assert_eq!(state.edid, "SSHQZone");
    assert_eq!(state.ecnz_flags, 1, "DATA: Flags");
    assert_ne!(state.ecnz_flags & 0x01, 0, "0x01 Never Resets");
    assert_eq!(
        state.ecnz_flags & 0x02,
        0,
        "0x02 Match PC Below Minimum Level"
    );
}

#[test]
fn an_encounter_zone_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = ECZN_HOOVER;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ECZN");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "ecnz_identity");

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
    let fid = ECZN_HOOVER;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ECZN");

    // DATA: the owner, the rank, the minimum level, the flags and the byte
    // the spec does not name.
    state.ecnz_owner = Some(0x0012_3456);
    state.ecnz_rank = -5;
    state.ecnz_minimum_level = 42;
    state.ecnz_flags |= 0x02;
    state.ecnz_unused = 0x7F;

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "ecnz_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 8, "DATA is eight bytes");
    assert_eq!(
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        0x0012_3456,
        "the edited owner"
    );
    assert_eq!(data[4] as i8, -5, "the edited rank");
    assert_eq!(data[5] as i8, 42, "the edited minimum level");
    assert_ne!(
        data[6] & 0x02,
        0,
        "the edited Match PC Below Minimum Level flag"
    );
    assert_eq!(data[7], 0x7F, "the edited unnamed byte");
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ECZN_HOOVER, *b"ECZN");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Encounter Zone - HooverDamZone");
}

#[test]
fn every_encounter_zone_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"ECZN", 17);
}
