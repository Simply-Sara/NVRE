//! What a drop does to the session, and where it lands.
//!
//! The placement rule is `Session::place` — the same path the viewport's drag
//! goes through — so this pins the two halves together: a drop writes a record
//! of the right type, at the position the drop computed, in the open cell.
//!
//! The world-dependent tests need the game's own masters, so they skip when
//! `NVRE_DATA` is unset, the way every other master-reading test here does.

use bevy::prelude::*;
use nvre_editor::object_drag::{bevy_point_to_game, placed_sig, DraggedObject};
use nvre_editor::session::Session;

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

fn open_master() -> Option<Session> {
    let data = data_dir()?;
    Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}

/// The record a drag writes, read back out of the staged insert.
fn placed(s: &Session) -> nvre_esm::Record {
    let p = s.doc.inserted().last().expect("the staged reference");
    p.record.clone()
}

#[test]
fn a_drop_writes_the_reference_the_signature_calls_for() {
    // `placed_sig` is the rule the session applies; this pins the two together so
    // a change to one is a change to both.
    assert_eq!(placed_sig(b"STAT"), *b"REFR");
    assert_eq!(placed_sig(b"NPC_"), *b"ACHR");
    assert_eq!(placed_sig(b"CREA"), *b"ACRE");
}

#[test]
fn a_drop_into_a_cell_is_staged_and_selected() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let cell = s
        .index
        .cells
        .values()
        .find(|c| c.edid.as_deref() == Some("Goodsprings"))
        .map(|c| c.form_id)
        .expect("Goodsprings");
    s.open_cell(cell);

    // A Bevy-space point, the way the viewport computes it, and the game-space
    // position the drop writes. The conversion is the drop's whole job.
    let bevy_point = Vec3::new(100.0, 0.0, -200.0);
    let game = bevy_point_to_game(bevy_point);

    let object = DraggedObject {
        form_id: s
            .object_groups("")
            .iter()
            .find(|g| g.category == "Static")
            .and_then(|g| g.objects.first())
            .map(|o| o.fid)
            .expect("a static"),
        sig: *b"STAT",
    };
    s.place(object.form_id, game);

    let rec = placed(&s);
    assert_eq!(
        rec.header.sig,
        placed_sig(&object.sig),
        "a static drops as a REFR"
    );
    // The position is the one the viewport computed, not a rounded or re-ordered
    // copy: the y and z swap is `bevy_to_game`, and getting it wrong is the
    // difference between a wall in the right place and a wall under the map.
    let subs = rec.subrecords().expect("subrecords");
    let data = subs
        .iter()
        .find(|s| &s.sig == b"DATA")
        .expect("DATA carries the placement");
    let f = |i: usize| f32::from_le_bytes(data.data[i * 4..i * 4 + 4].try_into().unwrap());
    assert_eq!(
        [f(0), f(1), f(2)],
        game,
        "the drop lands where it was aimed"
    );
    // And the base object is named, which is what makes it the dragged object.
    let name = subs
        .iter()
        .find(|s| &s.sig == b"NAME")
        .expect("NAME carries the base object");
    assert_eq!(
        u32::from_le_bytes(name.data[..4].try_into().unwrap()),
        object.form_id,
        "the reference names the object that was dragged"
    );
    assert_eq!(s.selected, Some(0), "the new reference is selected");
}

#[test]
fn a_drop_with_no_cell_open_is_refused() {
    // The viewport's drop reads the open cell; with none open there is nowhere to
    // put the reference, and the status line is what says so rather than a
    // silently dropped record.
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let before = s.pending_edits();
    s.place(0x0000_0001, [0.0, 0.0, 0.0]);
    assert_eq!(
        s.pending_edits(),
        before,
        "no cell open, so nothing is staged"
    );
    assert!(
        s.status.contains("open a cell"),
        "the status names the problem: {}",
        s.status
    );
}

#[test]
fn two_drops_are_two_references() {
    // A drag that places one object must not place two: the release frame is the
    // one that reads the drop, and the ghost is cleared on it, so a second read
    // of the same release has nothing left to place.
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let cell = s
        .index
        .cells
        .values()
        .find(|c| c.edid.as_deref() == Some("Goodsprings"))
        .map(|c| c.form_id)
        .expect("Goodsprings");
    s.open_cell(cell);
    let base = s
        .object_groups("")
        .iter()
        .find(|g| g.category == "Static")
        .and_then(|g| g.objects.first())
        .map(|o| o.fid)
        .expect("a static");

    s.place(base, [1.0, 2.0, 3.0]);
    s.place(base, [4.0, 5.0, 6.0]);
    assert_eq!(s.doc.inserted().len(), 2, "two drops, two references");
}
