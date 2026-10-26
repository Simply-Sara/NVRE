//! The cell browser's search box: what it matches.
//!
//! GECK's Cell View finds a cell by its Editor ID, and for most of FalloutNV
//! that is not enough — the bulk of the exterior cells have no Editor ID at all,
//! so they are findable only by FormID or by their grid square. The box sits next
//! to the "+ New worldspace" button, so it has to narrow the list with the same
//! text the table shows in it.

use nvre_editor::panels::row_matches;
use nvre_editor::session::{CellRow, CellScope, Session};

fn row(edid: &str, name: Option<&str>, form_id: u32, grid: Option<(i32, i32)>) -> CellRow {
    CellRow {
        form_id,
        edid: Some(edid.to_string()),
        name: name.map(str::to_string),
        interior: grid.is_none(),
        refs: 0,
        world: grid.is_some(),
        grid,
    }
}

#[test]
fn an_empty_box_keeps_every_row() {
    let c = row("TheFortSummit", Some("The Fort"), 0x0012F6B, None);
    assert!(row_matches(&c, ""), "an empty filter lists everything");
}

#[test]
fn the_editor_id_column_is_searched_case_insensitively() {
    let c = row("TheFortSummit", None, 0x0012F6B, None);
    assert!(row_matches(&c, "fort"));
    assert!(row_matches(&c, "FORTSUMMIT"));
    assert!(!row_matches(&c, "goodsprings"));
}

#[test]
fn the_display_name_column_is_searched_too() {
    // A cell whose Editor ID and display name differ: the name is the other
    // thing the user knows the cell by.
    let c = row(
        "CasaMadridSuite",
        Some("Casa Madrid Suite 1"),
        0x0012F6B,
        None,
    );
    assert!(row_matches(&c, "casa madrid"));
    assert!(row_matches(&c, "suite 1"));
    // And the Editor ID is still reachable, so a row is findable by either.
    assert!(row_matches(&c, "casamadrid"));
}

#[test]
fn the_form_id_column_is_searched_in_the_spelling_the_table_shows() {
    let c = row("00012F6B", None, 0x00012F6B, None);
    assert!(row_matches(&c, "00012F6B"), "the eight-digit spelling");
    assert!(row_matches(&c, "00012f6b"), "hex digits are not case-bound");
    // A cell with no Editor ID is findable only this way, which is most of the
    // exterior cells, so a FormID that matches nothing must not be dropped.
    assert!(!row_matches(&c, "FFFFFFFF"));
}

#[test]
fn the_grid_square_is_searched() {
    let c = row("WastelandNV", None, 0x0000C4A1, Some((-4, 7)));
    assert!(row_matches(&c, "-4, 7"), "the location column as shown");
    assert!(row_matches(&c, "-4,7"));
    assert!(!row_matches(&c, "4, 7"));
}

#[test]
fn a_real_cell_is_found_by_its_editor_id_its_name_and_its_form_id() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // A named interior, so all three columns hold something to search on.
    let named = s
        .cells(CellScope::Interiors)
        .into_iter()
        .find(|c| c.edid.is_some() && c.name.is_some())
        .expect("a named interior cell");
    assert!(row_matches(&named, named.edid.as_deref().unwrap()));
    assert!(row_matches(&named, named.name.as_deref().unwrap()));
    assert!(row_matches(&named, &format!("{:08X}", named.form_id)));
    // And an exterior cell, which has no Editor ID: the FormID and the grid
    // are what is left, and both have to work.
    let world = s
        .index
        .worldspaces
        .iter()
        .max_by_key(|(_, w)| w.grid.len())
        .map(|(&fid, _)| fid)
        .expect("a worldspace");
    let exterior = s
        .cells(CellScope::Worldspace(world))
        .into_iter()
        .find(|c| c.edid.is_none())
        .expect("an exterior cell with no Editor ID");
    assert!(row_matches(&exterior, &format!("{:08X}", exterior.form_id)));
    let (x, y) = exterior.grid.expect("an exterior grid");
    assert!(row_matches(&exterior, &format!("{x}, {y}")));
    // Its Editor ID column is the FormID, so that string is reachable twice.
    assert_eq!(exterior.editor_id(), format!("{:08X}", exterior.form_id));
}

fn open_master() -> Option<Session> {
    let data = std::env::var("NVRE_DATA").ok()?;
    Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}
