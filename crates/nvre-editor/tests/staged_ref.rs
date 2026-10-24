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

#[test]
fn a_placed_reference_names_its_base_object_without_reopening_the_cell() {
    // The references table resolves a row's Editor ID and Type from its base
    // object, and caches those answers per cell. `place` adds a row whose base
    // the cache does not know, so a stale cache shows the row as
    // "REFR 00000000" until the cell is reopened — the whole row is right except
    // the two columns that come from the cache. The cache has to be invalidated
    // by the placement, not only by the cell changing.
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
    // Build the cache *before* the placement, which is the state the bug needs:
    // a resolved map that does not yet contain the dragged object.
    let _ = s.ref_details().clone();

    let stat = s
        .object_groups("")
        .iter()
        .find(|g| g.category == "Static")
        .and_then(|g| g.objects.first())
        .map(|o| o.fid)
        .expect("a static");
    let stat_edid = s.ref_details().base_edid(stat).map(str::to_string);
    s.place(stat, [10.0, 20.0, 30.0]);

    // The row is in the cell's list, and the same cache now names it. Before the
    // fix this read returned the *pre-placement* build, whose `bases` map has no
    // entry for the dragged object: the Editor ID column fell back to the
    // reference's own signature and FormID, and the Type column went blank.
    let rows = s.cell_refs().to_vec();
    let row = rows
        .iter()
        .find(|r| r.form_id == 0)
        .expect("the staged row is in the cell's draw list");
    assert_eq!(base_of(row), Some(stat));
    let details = s.ref_details();
    assert!(
        stat_edid.is_none(),
        "the pre-placement build has no name for the dragged object: {stat_edid:?}"
    );
    assert!(
        details.base_edid(stat).is_some(),
        "the placed row's base resolves without the cell being reopened"
    );
    // And the Type column, which reads the same cache, is the schema's own name
    // for the base's record type rather than blank.
    assert_eq!(details.base_type(stat), Some("Static"));
    // The same base object is named for the row *without* the cell being
    // reopened: reopening would rebuild the cache and hide the bug.
    assert_eq!(s.cell, Some(cell), "the cell was not reopened to fix this");
}

#[test]
fn a_placed_reference_is_named_even_when_the_details_were_built_before_it() {
    // The other half of the same cache: the details are read *first*, then the
    // object is placed. The cache is per cell and the cell has not changed, so
    // without the generation it is served from the build that predates the drop
    // — the row's base is not in it and the Editor ID column falls back to the
    // signature and FormID. This is the reported symptom exactly.
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

    // The details are resolved while the dragged object is not yet in the cell.
    let before = s.ref_details().clone();
    let stat = s
        .object_groups("")
        .iter()
        .find(|g| g.category == "Static")
        .and_then(|g| g.objects.first())
        .map(|o| o.fid)
        .expect("a static");
    assert!(
        before.base_edid(stat).is_none(),
        "the pre-placement build does not hold the dragged object's base"
    );

    s.place(stat, [10.0, 20.0, 30.0]);

    // The *same* cache is asked again, without the cell being reopened.
    let after = s.ref_details();
    assert!(
        after.base_edid(stat).is_some(),
        "the details resolve the placed row's base without the cell reopening"
    );
    assert_eq!(s.cell, Some(cell), "the cell was not reopened to fix this");
}

#[test]
fn a_placed_reference_is_saved_with_the_formid_and_base_the_session_predicted() {
    // The other half of "does it set the FormID correctly": the session says
    // which FormID the staged record will carry (`selected_saved_id`), and the
    // save is what actually writes it. The two are independent code, so a test
    // that checks only one of them passes while the other drifts.
    //
    // The saved record's NAME is the base object's FormID *in the new plugin's
    // own numbering*: the dragged object belongs to a master, so its load-order
    // FormID is remapped to that master's index in the mod's master list.
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
    s.place(stat, [1234.0, 5678.0, 90.0]);
    let predicted = s.selected_saved_id().expect("the new record is selected");

    let out = std::env::temp_dir().join("nvre-editor-staged-ref-formid.esp");
    let report = s.save(&out).expect("the save writes");
    assert_eq!(report.inserted, 1, "one record was inserted");

    let saved = nvre_esm::Plugin::open(&out).expect("the saved plugin parses");
    let mut placed: Option<&nvre_esm::raw::Record> = None;
    nvre_esm::raw::visit_nodes(&saved.nodes, &mut |r, _| {
        if r.header.sig == *b"REFR" && placed.is_none() {
            placed = Some(r);
        }
    });
    let rec = placed.expect("the placed REFR is in the saved file");
    assert_eq!(
        rec.header.form_id, predicted,
        "the save wrote the FormID the session predicted"
    );
    // The FormID is the new plugin's own: the top byte is the master count.
    assert_eq!(
        rec.header.form_id >> 24,
        s.masters.len() as u32,
        "the FormID is in the new plugin's own range"
    );

    let subs = rec.subrecords().expect("subrecords");
    let name = subs
        .iter()
        .find(|x| &x.sig == b"NAME")
        .expect("NAME carries the base object");
    let named = u32::from_le_bytes(name.data[..4].try_into().unwrap());
    // NAME is remapped into the new plugin's own numbering: the dragged object
    // belongs to a master, so its load-order FormID's top byte becomes that
    // master's index in the mod's master list, and the low 24 bits are
    // untouched. Pinned by `formids_are_remapped_into_the_new_plugin` in
    // nvre-edit's own save tests, which measures the same rule.
    let master_index = s.masters.len() as u32;
    assert_eq!(
        named,
        ((master_index) << 24) | (stat & 0x00FF_FFFF),
        "NAME is the dragged object's FormID in the new plugin's numbering"
    );
    assert_eq!(
        named & 0x00FF_FFFF,
        stat & 0x00FF_FFFF,
        "the dragged object's own FormID is preserved"
    );

    // And the position the drop aimed at is what landed.
    let data_sub = subs
        .iter()
        .find(|x| &x.sig == b"DATA")
        .expect("DATA carries the placement");
    let f = |i: usize| f32::from_le_bytes(data_sub.data[i * 4..i * 4 + 4].try_into().unwrap());
    assert_eq!(
        [f(0), f(1), f(2)],
        [1234.0, 5678.0, 90.0],
        "the drop lands where it was aimed"
    );
}
