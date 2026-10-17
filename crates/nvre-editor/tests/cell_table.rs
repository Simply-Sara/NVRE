//! What the cell browser's table columns hold.
//!
//! The table shows Editor ID, Form ID, Name and Location. The last two are only
//! useful if the game's records carry them, and the two scopes carry different
//! ones: an interior cell is a named room, an exterior cell is a grid square.
//! So the columns are checked against the data that is actually there.

use nvre_editor::session::{CellScope, Session};

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

fn open_master() -> Option<Session> {
    let data = data_dir()?;
    Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../NVForever/schemas/fnv"
        )),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}

#[test]
fn an_interior_row_carries_a_name_and_says_it_is_inside() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let rows = s.cells(CellScope::Interiors);
    let named = rows.iter().filter(|r| r.name.is_some()).count();
    // Nearly every interior has a display name; the few that do not fall back
    // to their Editor ID, which is what the first column is for.
    assert!(
        named * 20 > rows.len() * 19,
        "an interior cell is a named room; {named} of {} had a name",
        rows.len()
    );
    // No interior has a grid coordinate, so the Location column must say so
    // rather than show a coordinate that means nothing indoors.
    assert!(
        rows.iter().all(|r| r.grid.is_none()),
        "an interior cell sits in no grid"
    );
    assert!(
        rows.iter().all(|r| r.location() == "interior"),
        "an interior's Location column says interior"
    );
}

#[test]
fn an_exterior_row_carries_its_grid_coordinate() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let world = s
        .index
        .worldspaces
        .iter()
        .max_by_key(|(_, w)| w.grid.len())
        .map(|(&fid, _)| fid)
        .expect("a worldspace");
    let rows = s.cells(CellScope::Worldspace(world));
    // An exterior cell's place in the world is its grid square, and every one
    // of them has one: without it the row shows nothing and the column is dead.
    assert!(
        rows.iter().all(|r| r.grid.is_some()),
        "an exterior cell's place is its grid square"
    );
    assert!(
        rows.iter().all(|r| r.location() != "interior"),
        "an exterior cell is not labelled interior"
    );
    // And the coordinate is readable as one, not as a placeholder.
    let (x, y) = rows[0].grid.expect("a grid coordinate");
    assert_eq!(rows[0].location(), format!("{x}, {y}"));
}

#[test]
fn the_editor_id_column_falls_back_to_the_form_id() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let world = s
        .index
        .worldspaces
        .iter()
        .max_by_key(|(_, w)| w.grid.len())
        .map(|(&fid, _)| fid)
        .expect("a worldspace");
    let rows = s.cells(CellScope::Worldspace(world));
    let no_edid = rows
        .iter()
        .find(|r| r.edid.is_none())
        .expect("a row with no EDID");
    // Most exterior cells have no Editor ID, so the first column shows the
    // FormID rather than an empty cell.
    assert_eq!(no_edid.editor_id(), format!("{:08X}", no_edid.form_id));
}
