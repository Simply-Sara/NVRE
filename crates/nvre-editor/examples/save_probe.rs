//! Does the editor's save path work on a plugin created from scratch, and on a
//! master opened directly? This is the check for "save failed: plugin does not
//! start with a TES4 record".
//!
//!   NVRE_DATA=<Data> cargo run -p nvre-editor --example save_probe -- <out.esp>

use nvre_edit::document::PendingRecord;
use nvre_edit::save::{write_plugin, MasterList};
use nvre_edit::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "SaveProbe.esp".into());
    let schema_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv");
    let schema = nvre_esm::Schema::load_dir(&schema_dir)?;
    let masters = MasterList::new(vec!["FalloutNV.esm".to_string()]);

    // 1. A plugin created from nothing, the way `Session::create` does.
    let mut blank = Document::create_empty("SaveProbe.esp", false, &["FalloutNV.esm".to_string()])?;
    blank.insert(PendingRecord {
        cell: 0x0031e2a,
        record: nvre_edit::document::make_refr_flags(
            0,
            0x0015_7C89,
            [1000.0, 2000.0, 300.0],
            [0.0, 0.0, 0.0],
            Some("SaveProbe01"),
            None,
            nvre_edit::document::REFR_PERSISTENT,
        ),
    });
    match write_plugin(&blank, &masters, &schema, std::path::Path::new(&out)) {
        Ok(r) => {
            let reparsed = nvre_esm::Plugin::open(&out)?;
            let first = match &reparsed.nodes[0] {
                nvre_esm::raw::Node::Record(rec) => rec.sig_str(),
                nvre_esm::raw::Node::Group(_) => "<a group>".to_string(),
            };
            println!(
                "blank plugin: OK {} bytes, {} records, first node {first}",
                r.bytes, r.records
            );
        }
        Err(e) => println!("blank plugin: FAILED: {e}"),
    }
    std::fs::remove_file(&out).ok();

    // 2. A master opened directly, which is the path that always worked.
    let doc = Document::open(format!("{data}/FalloutNV.esm"))?;
    match write_plugin(&doc, &masters, &schema, std::path::Path::new(&out)) {
        Ok(r) => println!("opened master: OK {} bytes, {} records", r.bytes, r.records),
        Err(e) => println!("opened master: FAILED: {e}"),
    }
    std::fs::remove_file(&out).ok();

    Ok(())
}
