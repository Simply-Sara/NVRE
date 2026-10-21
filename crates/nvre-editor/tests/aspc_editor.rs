//! The Ambient Sound (ASPC) editor reads and writes every field of every
//! subrecord in fopdoc's ASPC spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, `OBND`, five `SNAM` sound loops, `WNAM`, `RDAT`,
//! `ANAM` and `INAM`. The interesting part is the five `SNAM` loops: they are
//! *repeated* subrecords, so they must be written as a list or the record
//! loses four of its five loops.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 113 ASPC records, every one of them carrying five `SNAM` loops, a
//!   four-byte `WNAM`, a four-byte `ANAM` and a four-byte `INAM`.
//! - `ExtDesertNorthVegas` (0x0017AFB7): environment type 26 (Medium Room),
//!   and a walla loop that is null.
//! - 112 of the 113 records carry an `RDAT` naming a `REGN` record.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, payloads, saved_subs, sig_order,
    subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// ExtDesertNorthVegas (0x0017AFB7): five sound loops, a null walla.
const ASPC_DESERT: u32 = 0x0017_AFB7;

#[test]
fn every_field_of_a_real_ambient_sound_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ASPC_DESERT, *b"ASPC");

    assert_eq!(state.edid, "ExtDesertNorthVegas");

    // --- The five SNAM sound loops ---
    assert_eq!(
        state.aspc_sounds.len(),
        5,
        "SNAM: the record carries all five loops"
    );
    assert_eq!(state.aspc_sounds[0], 0x0016_9075, "SNAM: the dawn loop");
    assert_eq!(
        state.aspc_sounds[1], 0x0016_9073,
        "SNAM: the afternoon loop"
    );
    assert_eq!(state.aspc_sounds[2], 0x0016_9074, "SNAM: the dusk loop");
    assert_eq!(state.aspc_sounds[3], 0x0016_9076, "SNAM: the night loop");
    assert_eq!(state.aspc_sounds[4], 0, "SNAM: the walla loop is null");

    // --- WNAM: the walla trigger count ---
    assert_eq!(
        state.aspc_walla_trigger_count, 0,
        "WNAM: Walla Trigger Count"
    );

    // --- RDAT: the region ---
    assert_eq!(
        state.aspc_region,
        Some(0x0016_B451),
        "RDAT: Use Sound from Region"
    );

    // --- ANAM: the environment type ---
    assert_eq!(
        state.aspc_environment, 26,
        "ANAM: Environment Type (Medium Room)"
    );

    // --- INAM: is interior ---
    assert_eq!(state.aspc_is_interior, 0, "INAM: Is Interior");
}

#[test]
fn the_sound_loops_survive_a_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The five SNAM loops are *repeated* subrecords. Written with a
    // per-subrecord edit they would collapse onto the last one, and the
    // record would lose four of its five loops.
    let fid = ASPC_DESERT;
    let before = subs(&s, fid);
    assert_eq!(
        payloads(&before, b"SNAM").len(),
        5,
        "five loops before the save"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ASPC");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "aspc_loops");

    assert_eq!(
        payloads(&after, b"SNAM"),
        payloads(&before, b"SNAM"),
        "the five loops come back whole and in order"
    );
}

#[test]
fn an_ambient_sound_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = ASPC_DESERT;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ASPC");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "aspc_identity");

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
    let fid = ASPC_DESERT;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ASPC");

    // WNAM: the walla trigger count.
    state.aspc_walla_trigger_count = 7;

    // RDAT: the region.
    state.aspc_region = Some(0x0012_3456);

    // ANAM: the environment type.
    state.aspc_environment = 11; // Arena

    // INAM: is interior.
    state.aspc_is_interior = 2; // Yes

    // SNAM: the first sound loop.
    state.aspc_sounds[0] = 0x0012_3457;

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "aspc_edited");

    let u32_at = |sig: &[u8; 4]| -> u32 {
        let b = payload(&after, sig);
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    };
    assert_eq!(u32_at(b"WNAM"), 7, "WNAM: the edited trigger count");
    assert_eq!(u32_at(b"RDAT"), 0x0012_3456, "RDAT: the edited region");
    assert_eq!(u32_at(b"ANAM"), 11, "ANAM: the edited environment type");
    assert_eq!(u32_at(b"INAM"), 2, "INAM: the edited interior flag");

    let snam = payloads(&after, b"SNAM");
    assert_eq!(snam.len(), 5, "SNAM: the five loops are still there");
    assert_eq!(
        u32::from_le_bytes([snam[0][0], snam[0][1], snam[0][2], snam[0][3]]),
        0x0012_3457,
        "SNAM: the edited first loop"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ASPC_DESERT, *b"ASPC");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Ambient Sound - ExtDesertNorthVegas");
}

#[test]
fn every_ambient_sound_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"ASPC", 113);
}
