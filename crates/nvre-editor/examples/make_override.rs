//! Produce a mod that overrides an existing record, so NVForever's `fnvtool`
//! can check the override semantics.
//!
//! Run with:
//!   NVRE_DATA=<Data> cargo run -p nvre-editor --example make_override -- <out.esp>

use fnv_esm::Schema;
use nvre_edit::save::{write_plugin, MasterList};
use nvre_edit::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "MyOverride.esp".into());
    let schema_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../NVForever/schemas/fnv");
    let schema = Schema::load_dir(&schema_dir)?;

    // The mod depends on the master whose records it overrides.
    let masters = MasterList::new(vec!["FalloutNV.esm".to_string()]);

    let mut doc = Document::open(format!("{data}/FalloutNV.esm"))?;

    // 1. Move an existing placed reference: an override of its DATA subrecord.
    let moved = doc
        .records()
        .into_iter()
        .find(|(_, r)| {
            r.header.sig == *b"REFR"
                && r.subrecords()
                    .is_ok_and(|s| s.iter().any(|x| x.sig == *b"DATA"))
        })
        .map(|(f, _)| f)
        .expect("a REFR with DATA");
    let before =
        nvre_edit::document::value_of(&schema, doc.record(moved).unwrap(), b"DATA").unwrap();
    let mut new_value = before.clone();
    new_value["position"]["x"] = serde_json::json!(4242.0f64);
    let payload = nvre_edit::encode::encode_sub(&schema, b"REFR", b"DATA", &new_value)?;
    doc.set_sub(moved, b"DATA", payload);
    println!("override {moved:08X}: DATA.position.x -> 4242.0");

    // 2. Delete another placed reference: an override with the Deleted flag.
    let doomed = doc
        .records()
        .into_iter()
        .find(|(f, r)| *f != moved && r.header.sig == *b"REFR")
        .map(|(f, _)| f)
        .expect("another REFR");
    doc.delete_record(doomed);
    println!("delete   {doomed:08X}: Deleted flag");

    let report = write_plugin(&doc, &masters, &schema, std::path::Path::new(&out))?;
    println!("records         {}", report.records);
    println!("inserted        {}", report.inserted);
    println!("deleted         {}", report.deleted);
    println!("bytes           {}", report.bytes);
    for p in report.unremapped.iter().take(5) {
        println!("unremapped: {p}");
    }
    Ok(())
}
