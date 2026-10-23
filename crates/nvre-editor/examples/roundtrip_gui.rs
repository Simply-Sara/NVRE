//! Drive the GUI's session without a window: open a cell, place, move, delete,
//! and save — then hand the result to NVRE's own `fnvtool`.
//!
//! Run with:
//!   NVRE_DATA=<Data> cargo run -p nvre-editor --example roundtrip_gui -- <out.esp>

use nvre_editor::session::Session;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "GuiRoundtrip.esp".into());
    let schema = std::path::Path::new("schemas/fnv");
    let master = format!("{data}/FalloutNV.esm");

    let mut s = Session::open(
        std::path::Path::new(&master),
        schema,
        vec!["FalloutNV.esm".to_string()],
    )?;
    println!("loaded: {} records", s.doc.records().len());

    // Open Goodsprings.
    let cell = s
        .index
        .cells
        .values()
        .find(|c| c.edid.as_deref() == Some("Goodsprings"))
        .map(|c| c.form_id)
        .expect("Goodsprings");
    s.open_cell(cell);
    println!("opened {cell:08X}: {} refs", s.cell_refs().len());

    // Place a new reference from a base object in the load order.
    let lo = s.load_order()?;
    // The palette is sorted by label and truncated; take the whole set so the
    // search does not depend on where a name sorts.
    let base =
        nvre_editor::session::placeable_base_objects(lo, &s.strings, "NVGuardTower", usize::MAX)
            .into_iter()
            .find(|(_, label, _)| label.contains("NVGuardTower"))
            .map(|(f, _, _)| f)
            .expect("a STAT base object");
    s.place(base, [1000.0, 2000.0, 300.0]);
    println!("placed base {base:08X}");

    // Select it and nudge it. The staged record has FormID 0 until the save, so
    // its real ID is resolved through the session.
    let placed = s.selected_saved_id().expect("the new record is selected");
    s.nudge_selected([10.0, 0.0, 0.0]);
    println!(
        "nudged {placed:08X} (staged as {:08X})",
        s.selected.unwrap_or(0)
    );

    // Move an existing reference in the same cell.
    let existing = s.cell_refs().first().map(|r| r.form_id).expect("a ref");
    s.select(existing);
    s.nudge_selected([0.0, 20.0, 0.0]);
    println!("nudged existing {existing:08X}");

    // Delete one.
    let doomed = s
        .cell_refs()
        .get(1)
        .map(|r| r.form_id)
        .expect("another ref");
    s.select(doomed);
    s.delete_selected();
    println!("deleted {doomed:08X}");

    println!("pending edits: {}", s.pending_edits());
    let report = s.save(std::path::Path::new(&out))?;
    println!(
        "saved {} ({} records, {} bytes)",
        out, report.records, report.bytes
    );
    Ok(())
}
