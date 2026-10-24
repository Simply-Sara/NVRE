//! Make a real no-master plugin the way the launcher's Create-New tab does, put
//! a record in it, save it, and re-read the file with NVRE's own reader.
//!
//!   NVRE_DATA=<Data> cargo run -p nvre-editor --example no_master_mod -- <out.esp>
//!
//! Nothing here uses a master: the point is that a standalone plugin is a
//! plugin, not an error.

use nvre_editor::session::Session;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let out = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "NoMasterMod.esp".into()),
    );
    // The compiled-in schemas: the same ones a built editor uses.
    let schema_dir = std::path::PathBuf::new();

    let mut session = Session::create(
        out.file_name().unwrap().to_string_lossy().as_ref(),
        false,
        std::path::Path::new(&data),
        &schema_dir,
        Vec::new(),
    )?;
    println!(
        "created: masters={:?} plugins={} header.next_object_id=0x{:08X}",
        session.masters,
        session.plugins.len(),
        session.doc.plugin().header.next_object_id
    );

    // A worldspace of its own, an exterior cell in it, and a record in that
    // cell — everything a standalone plugin needs, with no reference to the
    // game's data at all.
    let world =
        session.create_worldspace("NVFNoMasterWorld", Some("Standalone".into()), 0.0, 0.0)?;
    println!("worldspace 0x{world:08X}");
    let cell = session.create_cell(
        "NVFNoMasterCell",
        Some("Standalone Cell".into()),
        Some(world),
        Some((0, 0)),
        false,
    )?;
    println!("cell 0x{cell:08X}");
    session.open_cell(cell);
    // A record whose base object is the cell's own worldspace record: a FormID
    // this file defines, so nothing points outside it.
    session.place(world, [0.0, 0.0, 100.0]);
    println!("status: {}", session.status);

    let report = session.save(&out)?;
    println!(
        "saved {}: {} records ({} inserted, {} cells, {} worldspaces), {} bytes",
        out.display(),
        report.records,
        report.inserted,
        report.added_cells,
        report.added_worldspaces,
        report.bytes
    );
    for p in report.unremapped.iter().take(10) {
        println!("  unremapped: {p}");
    }

    // Read the file back with NVRE's own reader, as an independent check.
    let saved = nvre_esm::Plugin::open(&out)?;
    println!(
        "reparsed: masters={:?} is_master={} next_object_id=0x{:08X}",
        saved.masters(),
        saved.header.is_master(),
        saved.header.next_object_id
    );
    let mut recs = Vec::new();
    nvre_esm::raw::visit_nodes(&saved.nodes, &mut |r, _| {
        if r.header.sig != *b"TES4" {
            recs.push(format!("{} {:08X}", r.sig_str(), r.header.form_id));
        }
    });
    println!("records: {recs:?}");
    let lo = nvre_esm::LoadOrder::new(std::slice::from_ref(&saved))?;
    println!(
        "as a load order of one: {} entries, {} unmappable",
        lo.index.len(),
        lo.unmappable.len()
    );
    // Every record must be the file's own, and above the reserved range.
    for (fid, _) in saved.records().iter().map(|r| (r.header.form_id, ())) {
        assert_eq!(fid >> 24, 0, "record {fid:08X} is not the file's own");
        assert!(
            fid & 0x00FF_FFFF >= 0x800,
            "record {fid:08X} is in the engine's reserved range"
        );
    }
    println!("every record is the file's own and clears the reserved range");
    Ok(())
}
