//! The record header's flag names are per game and per record type.
//!
//! The object editor's header used to render Fallout: New Vegas's bit 10 as
//! "Quest Item" for every game and every record type. It was right for FNV's
//! weapons and wrong everywhere else: Fallout 4's weapon names bit 2
//! "Non-Playable" and leaves bit 10 unnamed, and Skyrim's armour calls bit 15
//! "Visible When Distant" where Fallout 4 leaves it "Unknown 15".
//!
//! These assertions are the difference the tables have to keep, read through the
//! same lookup the header uses. They need no game data: the schemas are compiled
//! into the binary.

use nvre_editor::game::GameProfile;

/// The name a game gives a bit of a record type's header word.
fn bit_name(game: &str, sig: &str, bit: u32) -> Option<String> {
    let profile = GameProfile::by_id(game).expect("a known game");
    let schema = profile.load_schema().expect("the schema loads");
    let mut key = [0u8; 4];
    key.copy_from_slice(sig.as_bytes());
    schema
        .header_flags_for(&key)
        .iter()
        .find(|(b, _)| *b == bit)
        .map(|(_, name)| name.clone())
}

#[test]
fn fnv_weapon_bit_10_is_quest_item() {
    assert_eq!(
        bit_name("fnv", "WEAP", 10).as_deref(),
        Some("Quest Item"),
        "FNV names bit 10 of a weapon's header word Quest Item"
    );
}

#[test]
fn fo4_weapon_bit_10_is_unnamed() {
    // The bug the tables exist to fix: FNV's answer must not leak into FO4.
    assert_eq!(bit_name("fo4", "WEAP", 10), None);
    assert_eq!(
        bit_name("fo4", "WEAP", 2).as_deref(),
        Some("Non-Playable"),
        "FO4 names bit 2 of a weapon's header word Non-Playable"
    );
}

#[test]
fn the_same_bit_has_a_different_name_in_different_games() {
    // Skyrim's armour names bit 15; Fallout 4 leaves it unnamed. A single shared
    // table would have to pick one and be wrong in the other game.
    assert_eq!(
        bit_name("skyrim", "ARMO", 15).as_deref(),
        Some("Visible When Distant")
    );
    assert_eq!(bit_name("fo4", "ARMO", 15).as_deref(), Some("Unknown 15"));
}

#[test]
fn a_quote_in_a_flag_name_survives() {
    // xEdit writes "Can''t Wait" (Pascal doubles a quote); the doubled quote has
    // to come back as one character, not truncate the name at the apostrophe.
    assert_eq!(bit_name("fnv", "CELL", 19).as_deref(), Some("Can't Wait"));
}

#[test]
fn the_edition_inherits_its_family_flags() {
    // Skyrim LE and SE layer over `schemas/skyrim`, so they read the family's
    // table. A layer that restated the table would be a second copy to drift.
    // (`skyrim` is the Legendary Edition's profile; its version layer is
    // `schemas/skyrimle`.)
    for game in ["skyrim", "skyrimse"] {
        assert_eq!(
            bit_name(game, "ARMO", 15).as_deref(),
            Some("Visible When Distant"),
            "{game} reads the Skyrim family's header flags"
        );
    }
}

#[test]
fn a_record_type_with_no_table_has_no_named_flags() {
    // `GRAS` is a record xEdit names no header flags for. The header shows the
    // raw word and no checkboxes, rather than borrowing another type's names.
    assert!(
        bit_name("fnv", "GRAS", 0).is_none(),
        "a record type with no table must not invent flag names"
    );
}

#[test]
fn every_bit_in_every_table_is_inside_the_word() {
    // A bit past 31 cannot be set in a u32, so a table naming one would render a
    // checkbox that writes nothing.
    for id in ["fnv", "fo3", "skyrim", "fo4"] {
        let profile = GameProfile::by_id(id).expect("a known game");
        let schema = profile.load_schema().expect("the schema loads");
        for (sig, flags) in &schema.header_flags {
            assert_eq!(sig.len(), 4, "{id}: {sig:?} is not a signature");
            for (bit, name) in &flags.bits {
                assert!(*bit < 32, "{id} {sig} names bit {bit}, past the word");
                assert!(!name.trim().is_empty(), "{id} {sig} names bit {bit} \"\"");
            }
        }
    }
}
