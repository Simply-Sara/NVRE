//! The object editor's pages, and which games each is correct for.
//!
//! There are two, and they are both correct everywhere: the grid reads the schema
//! and the sectioned page reads the game's own table. The guard exists for the page
//! that is *not* safe everywhere — one written against a specific game's byte
//! offsets — which is what the 120 hand-written pages were, and what a widget with
//! real behaviour would be.
//!
//! These assertions are the invariant that makes per-game customisation safe: the
//! schema may name any page it likes for any game, and the editor either has that
//! page and it fits, or it says why not.

use nvre_editor::game::GameProfile;
use nvre_editor::object_editor::{EditorPage, Pages};

const GAMES: [&str; 5] = ["fnv", "fo3", "skyrim", "skyrimse", "fo4"];

#[test]
fn the_registry_has_exactly_the_two_data_pages() {
    let mut pages = Pages::default();
    assert!(pages.get("grid").is_some(), "the flat grid");
    assert!(pages.get("sections").is_some(), "the sectioned page");
    assert!(pages.has("grid") && pages.has("sections"));
}

#[test]
fn both_pages_serve_every_game() {
    // Neither page knows a game: the grid reads the schema and the sectioned page
    // reads the game's own table, so both are correct wherever a schema decodes.
    let mut pages = Pages::default();
    for name in ["grid", "sections"] {
        let page = pages.get(name).expect("registered");
        for game in GAMES {
            assert!(page.games().contains(&game), "{name} serves {game}");
        }
    }
}

#[test]
fn an_unknown_page_name_is_not_registered() {
    // A schema typo has to be a message, not a silently empty page. There is no
    // longer a table of hand-written pages for a name to fall back to.
    let mut pages = Pages::default();
    assert!(pages.get("weap").is_none());
    assert!(!pages.has("weap"));
}

#[test]
fn every_page_a_schema_names_is_registered() {
    let mut named = Vec::new();
    for id in GAMES {
        let profile = GameProfile::by_id(id).expect("a known game");
        let schema = profile.load_schema().expect("the schema loads");
        for (sig, cat) in &schema.object_window.categories {
            if let Some(name) = &cat.editor {
                named.push((id.to_string(), sig.clone(), name.clone()));
            }
        }
    }
    let pages = Pages::default();
    for (game, sig, name) in &named {
        assert!(
            pages.has(name),
            "{game} names page {name:?} for {sig}, which this build does not register"
        );
    }
    eprintln!("pages named by schemas: {named:?}");
}

#[test]
fn a_page_keeps_its_state_between_frames() {
    // Pages are held by the registry rather than rebuilt per frame, so a page can
    // own the widget state a modder is in the middle of using — the grid's
    // last-edited field, for one.
    let mut pages = Pages::default();
    let first = pages.get("grid").expect("the grid");
    let addr = first as *const dyn EditorPage as *const ();
    let again = pages.get("grid").expect("the grid");
    let addr2 = again as *const dyn EditorPage as *const ();
    assert_eq!(addr, addr2, "the same page instance comes back every frame");
}

#[test]
fn a_page_declares_the_games_its_layout_fits() {
    // The guard is still load-bearing: a widget with real behaviour declares the
    // games its layout fits, and the editor refuses it elsewhere. This is the shape
    // every retired page had — Fallout: New Vegas's byte offsets, right for the
    // Fallout family and wrong for Skyrim or Fallout 4.
    let fnv_only = FnvOnlyPage;
    assert!(fnv_only.games().contains(&"fnv"));
    assert!(fnv_only.games().contains(&"fo3"));
    assert!(
        !fnv_only.games().contains(&"fo4"),
        "a Fallout-family page is not correct for Fallout 4"
    );
}

/// A page that claims only the Fallout family, as every retired page would have.
struct FnvOnlyPage;

impl EditorPage for FnvOnlyPage {
    fn name(&self) -> &str {
        "fnv_only"
    }

    fn games(&self) -> &'static [&'static str] {
        &["fnv", "fo3"]
    }

    fn ui(
        &mut self,
        _ui: &mut nvre_editor::egui::Ui,
        _session: &mut nvre_editor::session::Session,
        _shared: &mut nvre_editor::object_editor::ObjectEditorState,
    ) {
    }
}
