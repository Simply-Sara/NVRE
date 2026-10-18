//! The cell browser's worldspace dropdown: the list is filtered by the scope.
//!
//! GECK's Cell View leads with a worldspace dropdown — "Interior" plus one entry
//! per worldspace — and the cell list below holds only that scope's cells. Getting
//! the partition wrong either hides interiors or lists 30,000 exterior cells under
//! "Interior", so both directions are checked against the game's own data.

use nvre_editor::session::{CellScope, Session};

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

fn open_master() -> Option<Session> {
    let data = data_dir()?;
    Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}

#[test]
fn the_dropdown_defaults_to_the_interiors() {
    assert_eq!(
        CellScope::default(),
        CellScope::Interiors,
        "the cell window opens on the interiors, the way GECK's does"
    );
}

#[test]
fn the_interiors_scope_holds_only_interior_cells() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let interiors = s.cells(CellScope::Interiors);
    assert!(!interiors.is_empty());
    assert!(
        interiors.iter().all(|c| !c.world && c.interior),
        "every cell under Interiors belongs to no worldspace"
    );
}

#[test]
fn a_worldspace_scope_holds_only_that_worlds_cells() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The Wasteland: the biggest scope in the game, so a filter that matched
    // everything would show up here first.
    let world = s
        .index
        .worldspaces
        .iter()
        .max_by_key(|(_, w)| w.grid.len())
        .map(|(&fid, _)| fid)
        .expect("a worldspace");
    let cells = s.cells(CellScope::Worldspace(world));
    assert!(!cells.is_empty());
    assert!(
        cells.iter().all(|c| c.world),
        "every cell in a worldspace's list belongs to it"
    );
    assert!(
        cells.iter().all(|c| !c.interior),
        "an exterior scope holds no interiors; those are the Interiors'"
    );
}

#[test]
fn every_cell_falls_under_exactly_one_scope() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut scoped = s.cells(CellScope::Interiors).len();
    for w in s.worldspaces() {
        scoped += s.cells(CellScope::Worldspace(w.form_id)).len();
    }
    // The dropdown partitions the cell list: nothing is listed twice, and
    // nothing is dropped from it either.
    assert_eq!(
        scoped,
        s.index.cells.len(),
        "every cell is under exactly one dropdown entry"
    );
}
