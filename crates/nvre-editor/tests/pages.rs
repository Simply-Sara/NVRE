//! The object editor's pages, and which games each is correct for.
//!
//! A page is a record type's layout. Fallout: New Vegas's weapon page decodes
//! Fallout: New Vegas's byte offsets; serving it Fallout 4's weapon of the same
//! signature would read garbage and show it as if it were the record, with
//! nothing saying so. So a page declares the games its layout fits, and the
//! object editor refuses to show it anywhere else.
//!
//! These assertions are the invariant that makes per-game customisation safe:
//! the schema may name any page it likes for any game, and the editor either has
//! that page and it fits, or it says why not.

use nvre_editor::game::GameProfile;
use nvre_editor::object_editor::Pages;

const GAMES: [&str; 5] = ["fnv", "fo3", "skyrim", "skyrimse", "fo4"];

/// Every page name every game's object-window table names, with the signature
/// that named it.
fn named_pages() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for id in GAMES {
        let profile = GameProfile::by_id(id).expect("a known game");
        let schema = profile.load_schema().expect("the schema loads");
        for (sig, cat) in &schema.object_window.categories {
            if let Some(name) = &cat.editor {
                out.push((id.to_string(), sig.clone(), name.clone()));
            }
        }
    }
    out
}

#[test]
fn a_hand_written_page_serves_only_the_games_its_layout_fits() {
    let mut pages = Pages::default();
    let weapon = pages.get("weap").expect("the weapon page is registered");
    for game in ["fnv", "fo3"] {
        assert!(
            weapon.games().contains(&game),
            "the weapon page is written against the Fallout family's offsets"
        );
    }
    for game in ["skyrim", "skyrimse", "fo4"] {
        assert!(
            !weapon.games().contains(&game),
            "{game}'s weapon is a different layout and the page must not claim it"
        );
    }
}

#[test]
fn the_grid_serves_every_game() {
    // The grid reads the schema and nothing else, so it is correct wherever the
    // schema decodes — which is the whole point of having it.
    let mut pages = Pages::default();
    let grid = pages.get("grid").expect("the grid is registered");
    for game in GAMES {
        assert!(grid.games().contains(&game), "the grid serves {game}");
    }
}

#[test]
fn an_unknown_page_name_is_not_registered() {
    // A schema typo has to be a message, not a silently empty page.
    let mut pages = Pages::default();
    assert!(pages.get("weapn").is_none());
    assert!(!pages.has("weapn"));
}

#[test]
fn every_page_a_schema_names_is_registered() {
    let named = named_pages();
    assert!(!named.is_empty(), "some game names pages");
    let pages = Pages::default();
    for (game, sig, name) in &named {
        assert!(
            pages.has(name),
            "{game} names page {name:?} for {sig}, which this build does not register"
        );
    }
}

#[test]
fn no_schema_names_a_page_for_a_game_it_does_not_serve() {
    // The guard. This fails the day a table names, say, the Fallout weapon page
    // for Fallout 4 — which is the mistake that would otherwise show a modder
    // the wrong fields with no warning at all.
    let named = named_pages();
    let mut pages = Pages::default();
    for (game, sig, name) in &named {
        let page = pages.get(name).expect("registered, checked above");
        assert!(
            page.games().contains(&game.as_str()),
            "{game} names page {name:?} for {sig}, and that page is only for {}",
            page.games().join(", ")
        );
    }
}

#[test]
fn a_page_keeps_its_state_between_frames() {
    // Pages are held by the registry rather than rebuilt per frame, which is what
    // lets a page own fields a modder is in the middle of editing.
    let mut pages = Pages::default();
    let first = pages.get("weap").expect("the weapon page");
    let addr = first as *const dyn nvre_editor::object_editor::EditorPage as *const ();
    let second = pages.get("weap").expect("the weapon page");
    let again = second as *const dyn nvre_editor::object_editor::EditorPage as *const ();
    assert_eq!(addr, again, "the same page instance comes back every frame");
}
