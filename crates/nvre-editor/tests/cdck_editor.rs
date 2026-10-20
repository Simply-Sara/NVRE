//! The Caravan Deck (CDCK) editor reads and writes every field of every
//! subrecord in fopdoc's CDCK spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, `FULL`, the `CARD` list and `DATA`. The `CARD`
//! subrecords are *repeated*, so the deck is a list of `CCRD` records.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 13 CDCK records, carrying between 30 and 60 `CARD` subrecords each
//!   (572 in all), and a four-byte `DATA`.
//! - `CaravanDeckNobark` (0x0017B41B): the first card is `0x00166F61` and
//!   the second is `0x00166EF2`.
//! - The spec calls `DATA` broken, so it is carried through unchanged rather
//!   than given a meaning the spec does not state. The game does write a
//!   real value: this deck's is 45.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, payloads, saved_subs, sig_order,
    subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// CaravanDeckNobark (0x0017B41B).
const CDCK_NOBARK: u32 = 0x0017_B41B;

#[test]
fn every_field_of_a_real_caravan_deck_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CDCK_NOBARK, *b"CDCK");

    assert_eq!(state.edid, "CaravanDeckNobark");
    assert_eq!(state.full_name, "Ringo's Caravan Deck");

    // --- The CARD list ---
    assert_eq!(state.cdck_cards.len(), 41, "CARD: the deck's cards");
    assert_eq!(state.cdck_cards[0], 0x0016_6F61, "CARD: the first card");
    assert_eq!(state.cdck_cards[1], 0x0016_6EF2, "CARD: the second card");

    // --- DATA: the value the spec calls broken ---
    // The spec marks the value broken, but the game writes a real one:
    // this deck's is 45. It is carried through unchanged.
    assert_eq!(state.cdck_data, 45, "DATA: the value the record had");
}

#[test]
fn the_card_list_survives_a_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The CARD subrecords are *repeated*. Written with a per-subrecord edit
    // they would collapse onto the last card, and the deck would lose
    // forty of its forty-one.
    let fid = CDCK_NOBARK;
    let before = subs(&s, fid);
    assert_eq!(
        payloads(&before, b"CARD").len(),
        41,
        "41 cards before the save"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CDCK");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "cdck_cards");

    assert_eq!(
        payloads(&after, b"CARD"),
        payloads(&before, b"CARD"),
        "the card list comes back whole and in order"
    );
}

#[test]
fn a_caravan_deck_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = CDCK_NOBARK;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CDCK");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "cdck_identity");

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
    let fid = CDCK_NOBARK;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CDCK");

    // CARD: the first card of the deck.
    state.cdck_cards[0] = 0x0012_3456;

    // DATA: the value the spec calls broken.
    state.cdck_data = 7;

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "cdck_edited");

    let cards = payloads(&after, b"CARD");
    assert_eq!(cards.len(), 41, "CARD: the deck's cards are still there");
    assert_eq!(
        u32::from_le_bytes([cards[0][0], cards[0][1], cards[0][2], cards[0][3]]),
        0x0012_3456,
        "CARD: the edited first card"
    );

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 4, "DATA is four bytes");
    assert_eq!(
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        7,
        "the edited DATA value"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CDCK_NOBARK, *b"CDCK");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(
        title,
        "Caravan Deck - CaravanDeckNobark (Ringo's Caravan Deck)"
    );
}

#[test]
fn every_caravan_deck_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"CDCK", 13);
}
