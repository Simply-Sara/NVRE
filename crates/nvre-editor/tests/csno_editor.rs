//! The Casino (CSNO) editor reads and writes every field of every subrecord
//! in fopdoc's CSNO spec, and keeps the record intact through a save.
//!
//! The spec lists `EDID`, `FULL`, a fifty-six-byte `DATA`, eight `MODL`
//! model paths, `MOD2`, `MOD3`, `MOD4`, seven `ICON` slot reel textures and
//! four `ICO2` blackjack deck textures. Every one of the model and texture
//! groups is a *repeated* subrecord, so each is written as a list.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 5 CSNO records, every one carrying a fifty-six-byte `DATA`, eight
//!   `MODL` values, one `MOD2`, one `MOD3`, one `MOD4`, seven `ICON` values
//!   and four `ICO2` values.
//! - `AtomicWranglerCasinoData` (0x00144C42): a decks-before-shuffle of
//!   0.35, a payout ratio of 1.2, seven reel stops of 2, one deck, a max
//!   winnings of 5000, the currency `0x00144C3E` and the flag "Dealer Stay
//!   on Soft 17".

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, payloads, saved_subs, sig_order,
    subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// AtomicWranglerCasinoData (0x00144C42).
const CSNO_ATOMIC: u32 = 0x0014_4C42;

#[test]
fn every_field_of_a_real_casino_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CSNO_ATOMIC, *b"CSNO");

    assert_eq!(state.edid, "AtomicWranglerCasinoData");
    assert_eq!(state.full_name, "Atomic Wrangler");

    // --- DATA: the two floats ---
    assert_eq!(
        state.csno_decks_shuffle, 0.35,
        "DATA: Decks % Before Shuffle"
    );
    assert_eq!(state.csno_payout_ratio, 1.2, "DATA: BlackJack Payout Ratio");

    // --- DATA: the seven reel stops ---
    assert_eq!(state.csno_reel_stops.len(), 7, "DATA: the reel stops");
    assert!(
        state.csno_reel_stops.iter().all(|s| *s == 2),
        "DATA: every reel stop is 2"
    );

    // --- DATA: the counts, the references and the flags ---
    assert_eq!(state.csno_number_of_decks, 1, "DATA: Number of Decks");
    assert_eq!(state.csno_max_winnings, 5000, "DATA: Max Winnings");
    assert_eq!(
        state.csno_currency,
        Some(0x0014_4C3E),
        "DATA: Currency (the CHIP record)"
    );
    assert_eq!(
        state.csno_winnings_quest,
        Some(0x0016_14E9),
        "DATA: Casino Winnings Quest"
    );
    assert_eq!(state.csno_flags, 1, "DATA: Flags");
    assert_ne!(state.csno_flags & 0x01, 0, "0x01 Dealer Stay on Soft 17");

    // --- The models ---
    assert_eq!(state.csno_models.len(), 11, "the model paths");
    assert_eq!(
        state.csno_models[0], "terminals\\NV_Blackjack\\NV_Blackjack-Chip_001.NIF",
        "the first chip model"
    );
    assert_eq!(
        state.csno_models[7], "terminals\\NV_SlotMachine\\NV_SlotMachine-Minigame-Generic.NIF",
        "the eighth model"
    );

    // --- The slot reel textures ---
    assert_eq!(state.csno_icons.len(), 7, "the slot reel textures");

    // --- The blackjack deck textures ---
    assert_eq!(
        state.csno_deck_icons.len(),
        4,
        "the blackjack deck textures"
    );
    assert_eq!(
        state.csno_deck_icons[0], "terminals\\NV_PlayingCards\\DeckB\\cardB_Back.dds",
        "the first deck texture"
    );
}

#[test]
fn the_model_and_texture_lists_survive_a_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The MODL, ICON and ICO2 groups are all *repeated* subrecords. Written
    // with a per-signature edit they would collapse onto their last entry.
    let fid = CSNO_ATOMIC;
    let before = subs(&s, fid);
    assert_eq!(
        payloads(&before, b"MODL").len(),
        8,
        "eight MODL values before"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CSNO");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "csno_lists");

    assert_eq!(
        payloads(&after, b"MODL"),
        payloads(&before, b"MODL"),
        "the MODL values come back whole and in order"
    );
    assert_eq!(
        payloads(&after, b"ICON"),
        payloads(&before, b"ICON"),
        "the ICON values come back whole and in order"
    );
    assert_eq!(
        payloads(&after, b"ICO2"),
        payloads(&before, b"ICO2"),
        "the ICO2 values come back whole and in order"
    );
    for sig in [*b"MOD2", *b"MOD3", *b"MOD4"] {
        assert_eq!(
            payload(&after, &sig),
            payload(&before, &sig),
            "{}: the numbered model comes back",
            nvre_util::tag_str(&sig)
        );
    }
}

#[test]
fn a_casino_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = CSNO_ATOMIC;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CSNO");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "csno_identity");

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
    let fid = CSNO_ATOMIC;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CSNO");

    // DATA: the two floats, the reel stops, the counts and the flags.
    state.csno_decks_shuffle = 0.5;
    state.csno_payout_ratio = 1.75;
    state.csno_reel_stops[0] = 9;
    state.csno_number_of_decks = 6;
    state.csno_max_winnings = 12345;
    state.csno_currency = Some(0x0012_3456);
    state.csno_winnings_quest = Some(0x0012_3457);
    state.csno_flags |= 0x01;

    // The models and the textures.
    state.csno_models[0] = "textures\\chip1.nif".to_string();
    state.csno_icons[0] = "textures\\symbol1.dds".to_string();
    state.csno_deck_icons[0] = "textures\\deck1.dds".to_string();

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "csno_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 56, "DATA keeps its fifty-six bytes");
    let f32_at =
        |at: usize| f32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    let u32_at =
        |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    assert_eq!(f32_at(0), 0.5, "the edited decks before shuffle");
    assert_eq!(f32_at(4), 1.75, "the edited payout ratio");
    assert_eq!(u32_at(8), 9, "the edited first reel stop");
    assert_eq!(u32_at(12), 2, "the other reel stops are unchanged");
    assert_eq!(u32_at(36), 6, "the edited number of decks");
    assert_eq!(u32_at(40), 12345, "the edited max winnings");
    assert_eq!(u32_at(44), 0x0012_3456, "the edited currency");
    assert_eq!(u32_at(48), 0x0012_3457, "the edited winnings quest");
    assert_eq!(u32_at(52) & 0x01, 0x01, "the edited flag");

    let modl = payloads(&after, b"MODL");
    assert_eq!(
        String::from_utf8_lossy(&modl[0]).trim_end_matches('\0'),
        "textures\\chip1.nif",
        "the edited first model"
    );
    let icon = payloads(&after, b"ICON");
    assert_eq!(
        String::from_utf8_lossy(&icon[0]).trim_end_matches('\0'),
        "textures\\symbol1.dds",
        "the edited first slot reel texture"
    );
    let ico2 = payloads(&after, b"ICO2");
    assert_eq!(
        String::from_utf8_lossy(&ico2[0]).trim_end_matches('\0'),
        "textures\\deck1.dds",
        "the edited first deck texture"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CSNO_ATOMIC, *b"CSNO");
    let title = nvre_editor::object_editor::window_title(&s.schema, &state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Casino - AtomicWranglerCasinoData (Atomic Wrangler)");
}

#[test]
fn every_casino_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"CSNO", 5);
}
