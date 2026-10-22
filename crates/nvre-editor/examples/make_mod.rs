//! Produce a real mod with the save path, so NVRE's own reader can check it.
//!
//! Run with:
//!   NVRE_DATA=<Data> cargo run -p nvre-editor --example make_mod -- <out.esp>

use nvre_esm::Schema;
use nvre_edit::document::PendingRecord;
use nvre_edit::save::{write_plugin, MasterList};
use nvre_edit::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "MyMod.esp".into());
    let schema_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv");
    let schema = Schema::load_dir(&schema_dir)?;

    // The mod depends on the base game and one DLC, in load order.
    let masters = MasterList::new(vec![
        "FalloutNV.esm".to_string(),
        "DeadMoney.esm".to_string(),
    ]);

    let mut doc = Document::open(format!("{data}/FalloutNV.esm"))?;

    // Insert two placed objects into WastelandNV's persistent cell.
    doc.insert(PendingRecord {
        cell: 0x0031e2a,
        record: nvre_edit::document::make_refr_flags(
            0,
            0x0015_7C89,
            [1000.0, 2000.0, 300.0],
            [0.0, 0.0, 0.0],
            Some("NVFModTest01"),
            None,
            nvre_edit::document::REFR_PERSISTENT,
        ),
    });
    doc.insert(PendingRecord {
        cell: 0x0031e2a,
        record: nvre_edit::document::make_refr_flags(
            0,
            0x0015_7C89,
            [1100.0, 2100.0, 300.0],
            [0.0, 0.0, 1.57],
            Some("NVFModTest02"),
            Some(1.5),
            nvre_edit::document::REFR_PERSISTENT,
        ),
    });

    let report = write_plugin(&doc, &masters, &schema, std::path::Path::new(&out))?;
    println!("records         {}", report.records);
    println!("inserted        {}", report.inserted);
    println!("remapped subs   {}", report.remapped_subs);
    println!("remapped formids{}", report.remapped_formids);
    println!("unremapped      {}", report.unremapped.len());
    for p in report.unremapped.iter().take(5) {
        println!("   {p}");
    }
    println!("bytes           {}", report.bytes);
    Ok(())
}
