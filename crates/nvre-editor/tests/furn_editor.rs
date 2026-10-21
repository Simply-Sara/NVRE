//! The Furniture (FURN) editor reads and writes every field of every
//! subrecord in fopdoc's FURN spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, `OBND`, `FULL`, the model data collection, `SCRI`,
//! the Destruction Data collection and `MNAM` (the marker flags).
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 234 FURN records, every one carrying a four-byte `MNAM`.
//! - `ControlConsole02NoAnim` (0x0017B7B7): the marker flags `01 00 00 00`
//!   and the model `NVDLC42\Clutter\Consoles\Console02NoAnim.NIF`.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// ControlConsole02NoAnim (0x0017B7B7).
const FURN_CONSOLE: u32 = 0x0017_B7B7;

/// ControlConsole03NoAnim (0x0017B7B6).
const FURN_CONSOLE3: u32 = 0x0017_B7B6;

#[test]
fn every_field_of_a_real_furniture_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, FURN_CONSOLE, *b"FURN");

    assert_eq!(state.edid, "ControlConsole02NoAnim");

    // --- MNAM: the marker flags ---
    assert_eq!(
        state.furn_marker_flags,
        vec![0x01, 0x00, 0x00, 0x00],
        "MNAM: the marker flags"
    );

    // --- The model data collection ---
    assert_eq!(
        state.furn_model.modl, "NVDLC42\\Clutter\\Consoles\\Console02NoAnim.NIF",
        "MODL: the model path"
    );
    assert_eq!(state.furn_model.modb, None, "MODB: this record has none");
    assert_eq!(state.furn_model.mods.len(), 0, "MODS: this record has none");
    assert_eq!(state.furn_model.modd, None, "MODD: this record has none");

    // --- The destruction data ---
    assert_eq!(
        state.furn_destruction.len(),
        0,
        "DEST: this record has no destruction data"
    );
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, FURN_CONSOLE, *b"FURN");
    assert_eq!(
        state.furn_model.modl,
        "NVDLC42\\Clutter\\Consoles\\Console02NoAnim.NIF"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, FURN_CONSOLE3, *b"FURN");
    assert_eq!(state.edid, "ControlConsole03NoAnim");
    assert_eq!(
        state.furn_model.modl, "NVDLC42\\Clutter\\Consoles\\ComputerRack01NoAnim.NIF",
        "MODL: this record's own model path"
    );
}

#[test]
fn a_furniture_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = FURN_CONSOLE;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"FURN");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "furn_identity");

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
    let fid = FURN_CONSOLE;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"FURN");

    // MNAM: the marker flags.
    state.furn_marker_flags = vec![0x02, 0x03, 0x04, 0x05];

    // MODL: the model path, through the header's Model Path field.
    state.model = "NVDLC42\\Clutter\\Consoles\\Console02.NIF".to_string();

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "furn_edited");

    let mnam = payload(&after, b"MNAM");
    assert_eq!(
        mnam,
        vec![0x02, 0x03, 0x04, 0x05],
        "the edited marker flags"
    );

    let modl = payload(&after, b"MODL");
    assert_eq!(
        String::from_utf8_lossy(&modl).trim_end_matches('\0'),
        "NVDLC42\\Clutter\\Consoles\\Console02.NIF",
        "the edited model path"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, FURN_CONSOLE, *b"FURN");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Furniture - ControlConsole02NoAnim");
}

#[test]
fn every_furniture_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"FURN", 234);
}
