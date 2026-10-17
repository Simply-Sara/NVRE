//! A reference's placement is read whole: position and rotation.
//!
//! Rotation used to be dropped, so 147 of Goodsprings' 159 references — roads,
//! doors, fences — were drawn axis-aligned. `fnvtool esm decode` is the
//! independent reader, and these are its numbers for two real records.

use nvre_editor::session::Session;

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

#[test]
fn placement_reads_position_and_rotation() {
    let Some(data) = data_dir() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut s = Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new("NVForever/schemas/fnv"),
        vec!["FalloutNV.esm".to_string()],
    )
    .expect("open the master");
    let cell = s
        .index
        .cells
        .values()
        .find(|c| c.edid.as_deref() == Some("Goodsprings"))
        .map(|c| c.form_id)
        .expect("Goodsprings");
    s.open_cell(cell);
    let refs = s.cell_refs().to_vec();

    // fnvtool's decode of 0x00104FB5: position (-71591.515625, 2312.631591796875,
    // 8365.7900390625), rotation (0.0, -0.0, 2.6226043701171875e-06).
    let r = refs
        .iter()
        .find(|r| r.form_id == 0x00104FB5)
        .expect("the reference");
    let p = r.position.expect("a position");
    assert!((p[0] - -71591.52).abs() < 0.01, "x: {}", p[0]);
    assert!((p[1] - 2312.6316).abs() < 0.01, "y: {}", p[1]);
    assert!((p[2] - 8365.79).abs() < 0.01, "z: {}", p[2]);
    let rot = r.rotation.expect("a rotation");
    assert!(rot[2].abs() < 1e-4, "z rotation: {}", rot[2]);

    // Most of a real cell is rotated. If this ever reads 0, the rotation is being
    // dropped again and the viewport draws everything axis-aligned.
    let rotated = refs
        .iter()
        .filter(|r| r.rotation.is_some_and(|x| x.iter().any(|a| a.abs() > 1e-4)))
        .count();
    assert!(
        rotated * 2 > refs.len(),
        "most of a cell's references are rotated; {rotated} of {} were",
        refs.len()
    );
}
