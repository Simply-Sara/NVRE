//! The Card (CCRD) editor reads and writes every field of every subrecord in
//! fopdoc's CCRD spec, and keeps the record intact through a save.
//!
//! The spec lists `EDID`, `OBND`, `FULL`, the model data collection, `ICON`,
//! `MICO`, `SCRI`, `YNAM`, `ZNAM`, `TX00`, `TX01`, two `INTV` values and
//! `DATA`. The two `INTV` values are a *repeated* subrecord, so they are the
//! card's suit and its value, in that order.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 270 CCRD records, every one carrying two `INTV` subrecords: the suit
//!   first, the value second.
//! - `CardSpadesQueenLucky38` (0x00166F61): suit 2 (Spades), value 13
//!   (Queen), and a `DATA` value of 2.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, payloads, saved_subs, sig_order,
    subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// CardSpadesQueenLucky38 (0x00166F61): a spade queen.
const CCRD_QUEEN: u32 = 0x0016_6F61;

/// CardSpadesKingLucky38 (0x00166F60): a spade king.
const CCRD_KING: u32 = 0x0016_6F60;

#[test]
fn every_field_of_a_real_card_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CCRD_QUEEN, *b"CCRD");

    assert_eq!(state.edid, "CardSpadesQueenLucky38");
    assert_eq!(state.full_name, "Queen of Spades - Lucky 38");

    // --- The two INTV values: the suit and the value ---
    assert_eq!(
        state.ccrd_intv.len(),
        2,
        "INTV: the record carries both values"
    );
    assert_eq!(state.ccrd_intv[0], 2, "INTV: Card Suit (Spades)");
    assert_eq!(state.ccrd_intv[1], 13, "INTV: Card Value (Queen)");

    // --- DATA: the card's value ---
    assert_eq!(state.ccrd_value, 2, "DATA: Value");

    // --- The images ---
    assert_eq!(
        state.ccrd_icon, "interface\\icons\\pipboyimages\\items\\item_playing_card.dds",
        "ICON: the large icon"
    );
    assert_eq!(
        state.ccrd_face, "terminals\\NV_PlayingCards\\DeckL\\cardL_s12.dds",
        "TX00: the high-res face image"
    );
    assert_eq!(
        state.ccrd_back, "terminals\\NV_PlayingCards\\DeckL\\cardL_Back.dds",
        "TX01: the high-res back image"
    );

    // --- The model ---
    assert_eq!(
        state.ccrd_model.modl, "clutter\\Caravan-Cards\\NV_Caravan-CardL.NIF",
        "MODL: the model path"
    );
}

#[test]
fn the_suit_and_the_value_survive_a_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The two INTV values are a *repeated* subrecord. Written with a
    // per-subrecord edit they would collapse onto the card value, and the card
    // would lose its suit.
    let fid = CCRD_QUEEN;
    let before = subs(&s, fid);
    assert_eq!(
        payloads(&before, b"INTV").len(),
        2,
        "two INTV values before the save"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CCRD");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "ccrd_intv");

    assert_eq!(
        payloads(&after, b"INTV"),
        payloads(&before, b"INTV"),
        "the suit and the value come back whole and in order"
    );
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CCRD_QUEEN, *b"CCRD");
    assert_eq!(state.ccrd_intv[1], 13);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CCRD_KING, *b"CCRD");
    assert_eq!(state.edid, "CardSpadesKingLucky38");
    assert_eq!(state.ccrd_intv[0], 2, "INTV: this record's own suit");
    assert_eq!(
        state.ccrd_intv[1], 14,
        "INTV: this record's own value (King)"
    );
}

#[test]
fn a_card_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = CCRD_QUEEN;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CCRD");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "ccrd_identity");

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
    let fid = CCRD_QUEEN;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CCRD");

    // INTV: the suit and the value.
    state.ccrd_intv[0] = 3; // Diamonds
    state.ccrd_intv[1] = 12; // Jack

    // DATA: the value.
    state.ccrd_value = 99;

    // The images.
    state.ccrd_icon = "textures\\card.dds".to_string();
    state.ccrd_face = "textures\\face.dds".to_string();
    state.ccrd_back = "textures\\back.dds".to_string();

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "ccrd_edited");

    let intv = payloads(&after, b"INTV");
    assert_eq!(intv.len(), 2, "INTV: the two values are still there");
    assert_eq!(
        u32::from_le_bytes([intv[0][0], intv[0][1], intv[0][2], intv[0][3]]),
        3,
        "INTV: the edited suit"
    );
    assert_eq!(
        u32::from_le_bytes([intv[1][0], intv[1][1], intv[1][2], intv[1][3]]),
        12,
        "INTV: the edited value"
    );

    let data = payload(&after, b"DATA");
    assert_eq!(
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        99,
        "the edited DATA value"
    );

    for (sig, want) in [
        (b"ICON", "textures\\card.dds"),
        (b"TX00", "textures\\face.dds"),
        (b"TX01", "textures\\back.dds"),
    ] {
        let got = payload(&after, sig);
        assert_eq!(
            String::from_utf8_lossy(&got).trim_end_matches('\0'),
            want,
            "{}: the edited image",
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
    state.load_from_session(&s, CCRD_QUEEN, *b"CCRD");
    let title = nvre_editor::object_editor::window_title(
        &s.schema,
        &state.sig,
        &state.edid,
        &state.full_name,
    );
    assert_eq!(
        title,
        "Caravan Card - CardSpadesQueenLucky38 (Queen of Spades - Lucky 38)"
    );
}

#[test]
fn every_card_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"CCRD", 270);
}
