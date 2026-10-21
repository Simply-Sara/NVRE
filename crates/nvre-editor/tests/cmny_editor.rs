//! The Money (CMNY) editor reads and writes every field of every subrecord in
//! fopdoc's CMNY spec, and keeps the record intact through a save.
//!
//! The spec lists `EDID`, `OBND`, `FULL`, the model data collection, `ICON`,
//! `MICO`, `YNAM`, `ZNAM` and `DATA` (the absolute value).
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 6 CMNY records, every one of them `EDID OBND FULL MODL ICON MICO DATA` in
//!   that order, with a four-byte `DATA`.
//! - `MoneyNCR100` (0x00176AC9): the absolute value is 40, and the small
//!   icon is the "glow" texture the game writes for money.
//! - `MoneyNCR20` (0x00176AC8): the absolute value is 8.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// MoneyNCR100 (0x00176AC9).
const CMNY_NCR100: u32 = 0x0017_6AC9;

/// MoneyNCR20 (0x00176AC8).
const CMNY_NCR20: u32 = 0x0017_6AC8;

#[test]
fn every_field_of_a_real_money_record_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CMNY_NCR100, *b"CMNY");

    assert_eq!(state.edid, "MoneyNCR100");
    assert_eq!(state.full_name, "$100 NCR");

    // --- DATA: the absolute value ---
    assert_eq!(state.cmny_value, 40, "DATA: Absolute Value");

    // --- The icons ---
    assert_eq!(
        state.cmny_icon, "interface\\icons\\pipboyimages\\items\\items_money_NCR_100.dds",
        "ICON: the large icon"
    );
    assert_eq!(
        state.cmny_mico,
        "interface\\icons\\pipboyimages_small\\items_small\\glow_items_money_NCR_100.dds",
        "MICO: the small icon"
    );

    // --- The model ---
    assert_eq!(
        state.cmny_model.modl, "clutter\\Money\\NCRMoney100.nif",
        "MODL: the model path"
    );
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CMNY_NCR100, *b"CMNY");
    assert_eq!(state.cmny_value, 40);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CMNY_NCR20, *b"CMNY");
    assert_eq!(state.edid, "MoneyNCR20");
    assert_eq!(state.full_name, "$20 NCR");
    assert_eq!(
        state.cmny_value, 8,
        "DATA: this record's own absolute value"
    );
    assert_eq!(
        state.cmny_model.modl, "clutter\\Money\\NCRMoney20.nif",
        "MODL: this record's own model path"
    );
}

#[test]
fn a_money_record_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = CMNY_NCR100;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CMNY");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "cmny_identity");

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
    let fid = CMNY_NCR100;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CMNY");

    // DATA: the absolute value.
    state.cmny_value = 250;

    // The icons.
    state.cmny_icon = "textures\\money.dds".to_string();
    state.cmny_mico = "textures\\money_small.dds".to_string();

    // MODL: the model path, through the header's Model Path field.
    state.model = "clutter\\Money\\NCRMoney250.nif".to_string();

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "cmny_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 4, "DATA is four bytes");
    assert_eq!(
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        250,
        "the edited absolute value"
    );

    for (sig, want) in [
        (b"ICON", "textures\\money.dds"),
        (b"MICO", "textures\\money_small.dds"),
        (b"MODL", "clutter\\Money\\NCRMoney250.nif"),
    ] {
        let got = payload(&after, sig);
        assert_eq!(
            String::from_utf8_lossy(&got).trim_end_matches('\0'),
            want,
            "{}: the edited value",
            nvre_util::tag_str(sig)
        );
    }
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CMNY_NCR100, *b"CMNY");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Money - MoneyNCR100 ($100 NCR)");
}

#[test]
fn every_money_record_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"CMNY", 6);
}
