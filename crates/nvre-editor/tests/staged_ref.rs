//! What a staged (unsaved) reference looks like to the viewport.
//!
//! A dropped object is staged with FormID 0 and lives in the document's pending
//! inserts, not in the plugin. The viewport reaches it through
//! `Session::cell_refs`, which appends those inserts to the cell's rows — and it
//! is only drawn if its base object resolves to a model path. This pins each
//! step of that chain against a real record, because the failure is silent: a
//! staged ref that resolves nothing is skipped with no error and no model.

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

/// The base FormID a reference's NAME names, as the viewport parses it.
fn base_of(row: &nvre_editor::session::RefRow) -> Option<u32> {
    let b = row.base.as_deref()?;
    u32::from_str_radix(b.strip_prefix("0x")?, 16).ok()
}

#[test]
fn a_staged_reference_carries_a_resolvable_base() {
    // The chain the viewport walks: row.base is "0x…" text, that parses to a
    // FormID, and the session resolves that FormID to a model path. If the base
    // is `None` or unparseable, the ref is counted as `no_base` and skipped.
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

    let stat = s
        .object_groups("")
        .iter()
        .find(|g| g.category == "Static")
        .and_then(|g| g.objects.first())
        .map(|o| o.fid)
        .expect("a static");
    s.place(stat, [100.0, 200.0, 300.0]);

    let staged = s
        .doc
        .inserted()
        .iter()
        .find(|p| p.cell == cell)
        .expect("the staged reference");
    // The record is a REFR, it carries NAME, and NAME is the base object.
    assert_eq!(staged.record.header.sig, *b"REFR");
    let subs = staged.record.subrecords().expect("subrecords");
    let name = subs
        .iter()
        .find(|x| &x.sig == b"NAME")
        .expect("a NAME subrecord");
    let named = u32::from_le_bytes(name.data[..4].try_into().unwrap());
    assert_eq!(named, stat, "NAME names the base object that was dragged");

    // And the row the viewport sees carries that base as parseable text.
    let rows = s.cell_refs().to_vec();
    let row = rows
        .iter()
        .find(|r| r.form_id == 0)
        .expect("the staged row is in the cell's draw list");
    assert_eq!(
        base_of(row),
        Some(stat),
        "the staged row's base resolves to the dragged object"
    );
    // The position survived, which is what makes the drop land where it was aimed.
    assert_eq!(row.position, Some([100.0, 200.0, 300.0]));
    // And the base resolves to a model path — the step that actually decides
    // whether anything is drawn.
    let path = s.model_path_for(stat);
    assert!(
        path.is_some(),
        "the base object resolves to a model; got {path:?}"
    );
}

#[test]
fn a_staged_reference_is_counted_in_the_cell() {
    // A staged record that never appears in `cell_refs` is invisible to the
    // viewport even when everything else is right, which is exactly the
    // "placed but not rendered" symptom.
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
    let before = s.cell_refs().len();

    let stat = s
        .object_groups("")
        .iter()
        .find(|g| g.category == "Static")
        .and_then(|g| g.objects.first())
        .map(|o| o.fid)
        .expect("a static");
    s.place(stat, [10.0, 20.0, 30.0]);

    assert_eq!(
        s.cell_refs().len(),
        before + 1,
        "the staged reference is in the cell's rows"
    );
}
