//! Creating cells and worldspaces from the editor's session.
//!
//! The session is what the Cells window talks to, so these tests drive the
//! same calls the panel makes and check that a cell staged through the
//! session lands in a group tree the engine can find.

use nvre_editor::session::Session;

/// The game's Data folder, when the environment names one.
fn data_dir() -> Option<std::path::PathBuf> {
    std::env::var("NVRE_DATA")
        .ok()
        .map(std::path::PathBuf::from)
}

fn schema_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")
}

/// Open the master as a session, the way the editor does.
fn session() -> Option<Session> {
    let data = data_dir()?;
    let plugin = data.join("FalloutNV.esm");
    Session::open(&plugin, &schema_dir(), vec!["FalloutNV.esm".to_string()]).ok()
}

/// A new interior cell is staged, and the session says how many it has.
#[test]
fn a_new_interior_cell_is_staged() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let before = s.staged_cells().len();
    let fid = s
        .create_cell("NvreSessionInterior", None, None, None, false)
        .expect("the cell is created");
    assert_eq!(
        s.staged_cells().len(),
        before + 1,
        "the cell must be staged"
    );
    assert_eq!(
        s.staged_cells().last().unwrap().spec.edid,
        "NvreSessionInterior"
    );
    // The FormID is the plugin's own, so its top byte is the plugin's own
    // index in its own master list.
    assert_eq!(
        (fid >> 24) as u8,
        s.doc.plugin().self_index(),
        "the new cell's FormID must be in the plugin's own range"
    );
}

/// A cell with no Editor ID is refused.
#[test]
fn a_cell_without_an_editor_id_is_refused() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let before = s.staged_cells().len();
    assert!(
        s.create_cell("  ", None, None, None, false).is_err(),
        "a cell with no Editor ID must be refused"
    );
    assert_eq!(s.staged_cells().len(), before, "nothing may be staged");
}

/// An exterior cell in a worldspace that does not exist is refused.
#[test]
fn an_exterior_cell_in_an_unknown_worldspace_is_refused() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let before = s.staged_cells().len();
    assert!(
        s.create_cell("NvreBadWorld", None, Some(0x7FFF_FFFF), Some((0, 0)), true)
            .is_err(),
        "a cell in a worldspace that does not exist must be refused"
    );
    assert_eq!(s.staged_cells().len(), before, "nothing may be staged");
}

/// A new worldspace is staged, and a cell can be put in it.
#[test]
fn a_new_worldspace_and_a_cell_in_it_are_staged() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let world = s
        .create_worldspace("NvreSessionWorld", None, -2500.0, -2300.0)
        .expect("the worldspace is created");
    assert_eq!(
        s.staged_worldspaces().len(),
        1,
        "the worldspace must be staged"
    );

    // A cell in the new worldspace, at a grid square.
    let cell = s
        .create_cell("NvreSessionExterior", None, Some(world), Some((1, 1)), true)
        .expect("the cell is created");
    assert_eq!(s.staged_cells().len(), 1, "the cell must be staged");
    let staged = &s.staged_cells()[0];
    assert_eq!(
        staged.spec.world,
        Some(world),
        "the cell names the worldspace"
    );
    assert_eq!(staged.spec.grid, Some((1, 1)), "the cell carries the grid");
    assert!(staged.spec.with_land, "the cell was given terrain");
    assert!(
        staged.land_form_id.is_some(),
        "the cell's terrain must have a FormID"
    );
    // The terrain's FormID is not the cell's.
    assert_ne!(staged.land_form_id, Some(cell));
}

/// Two new cells do not share a FormID.
#[test]
fn two_new_cells_do_not_share_a_form_id() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let a = s
        .create_cell("NvreFirst", None, None, None, false)
        .expect("the first cell");
    let b = s
        .create_cell("NvreSecond", None, None, None, false)
        .expect("the second cell");
    assert_ne!(a, b, "two cells must not share a FormID");
}
