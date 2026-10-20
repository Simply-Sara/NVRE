//! The Challenge (CHAL) editor reads and writes every field of every
//! subrecord in fopdoc's CHAL spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, `FULL`, `ICON`, `MICO`, `SCRI`, `DESC`, a
//! twenty-four-byte `DATA` and the two reference values `SNAM` and `XNAM`.
//! The meaning of `DATA`'s three value fields depends on the type, so they
//! are kept as raw bytes.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 105 CHAL records, every one carrying a twenty-four-byte `DATA`.
//! - `NVChallengeCaravanMaster` (0x0016BA4E): type 11 (Miscellaneous Stat),
//!   threshold 27, flags 1 (Start Disabled), interval 10.
//! - `NVChallengeOldTyme` (0x001690E8): type 8 (Do Damage), threshold 10000,
//!   flags 0, interval 2500, and a non-zero Value2 of `[45, 0]`.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// NVChallengeCaravanMaster (0x0016BA4E).
const CHAL_CARAVAN: u32 = 0x0016_BA4E;

/// NVChallengeOldTyme (0x001690E8).
const CHAL_OLDTYME: u32 = 0x0016_90E8;

#[test]
fn every_field_of_a_real_challenge_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CHAL_CARAVAN, *b"CHAL");

    assert_eq!(state.edid, "NVChallengeCaravanMaster");
    assert_eq!(state.full_name, "Know When to Hold 'Em");

    // --- DATA: the type, the threshold, the flags and the interval ---
    assert_eq!(state.chal_type, 11, "DATA: Type (Miscellaneous Stat)");
    assert_eq!(state.chal_threshold, 27, "DATA: Threshold");
    assert_eq!(state.chal_flags, 1, "DATA: Flags");
    assert_ne!(state.chal_flags & 0x01, 0, "0x01 Start Disabled");
    assert_eq!(state.chal_flags & 0x02, 0, "0x02 Recurring is clear");
    assert_eq!(
        state.chal_flags & 0x04,
        0,
        "0x04 Show Zero Progress is clear"
    );
    assert_eq!(state.chal_interval, 10, "DATA: Interval");

    // --- DATA: the three value fields ---
    assert_eq!(state.chal_value1, [37, 0], "DATA: Value1");
    assert_eq!(state.chal_value2, [0, 0], "DATA: Value2");
    assert_eq!(state.chal_value3, [0, 0, 0, 0], "DATA: Value3");

    // --- DESC: the description ---
    assert_eq!(state.chal_desc, "Win at Caravan.", "DESC: the description");

    // --- The reference values ---
    assert_eq!(state.chal_snam, None, "SNAM: this record has none");
    assert_eq!(state.chal_xnam, None, "XNAM: this record has none");
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CHAL_CARAVAN, *b"CHAL");
    assert_eq!(state.chal_value1, [37, 0]);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CHAL_OLDTYME, *b"CHAL");
    assert_eq!(state.edid, "NVChallengeOldTyme");
    assert_eq!(
        state.chal_type, 8,
        "DATA: this record's own type (Do Damage)"
    );
    assert_eq!(
        state.chal_threshold, 10000,
        "DATA: this record's own threshold"
    );
    assert_eq!(state.chal_flags, 0, "DATA: this record's own flags");
    assert_eq!(
        state.chal_interval, 2500,
        "DATA: this record's own interval"
    );
    assert_eq!(state.chal_value2, [45, 0], "DATA: this record's own Value2");
    assert_eq!(state.chal_value1, [0, 0], "DATA: this record's own Value1");
}

#[test]
fn a_challenge_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = CHAL_CARAVAN;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CHAL");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "chal_identity");

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
    let fid = CHAL_CARAVAN;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CHAL");

    // DATA: the type, the threshold, the flags, the interval and the values.
    state.chal_type = 3;
    state.chal_threshold = 500;
    state.chal_flags |= 0x02; // Recurring
    state.chal_interval = 60;
    state.chal_value1 = [1, 2];
    state.chal_value2 = [3, 4];
    state.chal_value3 = [5, 6, 7, 8];

    // The reference values.
    state.chal_snam = Some(0x0012_3456);
    state.chal_xnam = Some(0x0012_3457);

    // DESC: the description.
    state.chal_desc = "Edited description".to_string();

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "chal_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 24, "DATA is twenty-four bytes");
    let u32_at =
        |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    assert_eq!(u32_at(0), 3, "the edited type");
    assert_eq!(u32_at(4), 500, "the edited threshold");
    assert_eq!(u32_at(8) & 0x02, 0x02, "the edited Recurring flag");
    assert_eq!(u32_at(8) & 0x01, 0x01, "the other flags survive");
    assert_eq!(u32_at(12), 60, "the edited interval");
    assert_eq!(&data[16..18], &[1, 2], "the edited Value1");
    assert_eq!(&data[18..20], &[3, 4], "the edited Value2");
    assert_eq!(&data[20..24], &[5, 6, 7, 8], "the edited Value3");

    let snam = payload(&after, b"SNAM");
    assert_eq!(
        u32::from_le_bytes([snam[0], snam[1], snam[2], snam[3]]),
        0x0012_3456,
        "the edited SNAM"
    );
    let xnam = payload(&after, b"XNAM");
    assert_eq!(
        u32::from_le_bytes([xnam[0], xnam[1], xnam[2], xnam[3]]),
        0x0012_3457,
        "the edited XNAM"
    );

    let desc = payload(&after, b"DESC");
    assert_eq!(
        String::from_utf8_lossy(&desc).trim_end_matches('\0'),
        "Edited description",
        "the edited description"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CHAL_CARAVAN, *b"CHAL");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(
        title,
        "Challenge - NVChallengeCaravanMaster (Know When to Hold 'Em)"
    );
}

#[test]
fn every_challenge_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"CHAL", 105);
}
