//! The Casino Chip (CHIP) editor reads and writes every field of every
//! subrecord in fopdoc's CHIP spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, `OBND`, `FULL`, the model data collection, `ICON`,
//! `MICO`, the Destruction Data collection, `YNAM` and `ZNAM`.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 5 CHIP records, every one of them `EDID OBND FULL MODL ICON` in that
//!   order. None of them carries the destruction data or either sound.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// AtomicWranglerChip (0x00144C3E): real bounds and a distinct icon.
const CHIP_ATOMIC: u32 = 0x0014_4C3E;

/// VikkiVanceChip (0x00144C3D): zero bounds.
const CHIP_VIKKI: u32 = 0x0014_4C3D;

#[test]
fn every_field_of_a_real_casino_chip_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CHIP_ATOMIC, *b"CHIP");

    assert_eq!(state.edid, "AtomicWranglerChip");
    assert_eq!(state.full_name, "Atomic Wrangler Chip");

    // --- The icons ---
    assert_eq!(
        state.chip_icon, "interface\\icons\\pipboyimages\\items\\item_pokerchip_atomic.dds",
        "ICON: the large icon"
    );
    assert_eq!(state.chip_mico, "", "MICO: this record has none");

    // --- The model ---
    assert_eq!(
        state.chip_model.modl, "clutter\\casino\\PokerChip03.nif",
        "MODL: the model path"
    );

    // --- The destruction data ---
    assert_eq!(
        state.chip_destruction.len(),
        0,
        "DEST: this record has none"
    );
    assert_eq!(state.chip_dest_health, 0, "DEST: Health");

    // --- The sounds ---
    assert_eq!(state.chip_pickup_sound, None, "YNAM: this record has none");
    assert_eq!(state.chip_drop_sound, None, "ZNAM: this record has none");
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CHIP_ATOMIC, *b"CHIP");
    assert_eq!(
        state.chip_icon,
        "interface\\icons\\pipboyimages\\items\\item_pokerchip_atomic.dds"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CHIP_VIKKI, *b"CHIP");
    assert_eq!(state.edid, "VikkiVanceChip");
    assert_eq!(
        state.chip_icon, "interface\\icons\\pipboyimages\\items\\item_pokerchip_vikkivance.dds",
        "ICON: this record's own icon"
    );
}

#[test]
fn a_casino_chip_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = CHIP_ATOMIC;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CHIP");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "chip_identity");

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
    let fid = CHIP_ATOMIC;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CHIP");

    // ICON: the large icon.
    state.chip_icon = "textures\\chip.dds".to_string();

    // MODL: the model path, through the header's Model Path field.
    state.model = "clutter\\casino\\PokerChip04.nif".to_string();

    // The sounds.
    state.chip_pickup_sound = Some(0x0012_3456);
    state.chip_drop_sound = Some(0x0012_3457);

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "chip_edited");

    let icon = payload(&after, b"ICON");
    assert_eq!(
        String::from_utf8_lossy(&icon).trim_end_matches('\0'),
        "textures\\chip.dds",
        "the edited icon"
    );

    let modl = payload(&after, b"MODL");
    assert_eq!(
        String::from_utf8_lossy(&modl).trim_end_matches('\0'),
        "clutter\\casino\\PokerChip04.nif",
        "the edited model path"
    );

    let ynam = payload(&after, b"YNAM");
    assert_eq!(
        u32::from_le_bytes([ynam[0], ynam[1], ynam[2], ynam[3]]),
        0x0012_3456,
        "the edited pick-up sound"
    );
    let znam = payload(&after, b"ZNAM");
    assert_eq!(
        u32::from_le_bytes([znam[0], znam[1], znam[2], znam[3]]),
        0x0012_3457,
        "the edited drop sound"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CHIP_ATOMIC, *b"CHIP");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(
        title,
        "Casino Chip - AtomicWranglerChip (Atomic Wrangler Chip)"
    );
}

#[test]
fn every_casino_chip_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"CHIP", 5);
}
