//! Time the per-frame work the viewport and panels do, so the cost is known
//! rather than guessed.
//!
//! NVRE_DATA=<Data> cargo run --release -p nvre-editor --example bench_frames -- Goodsprings N

use nvre_editor::session::Session;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let cell = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Goodsprings".into());
    let frames: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    let schema = std::path::Path::new("NVForever/schemas/fnv");
    let master = format!("{data}/FalloutNV.esm");

    let mut s = Session::open(
        std::path::Path::new(&master),
        schema,
        vec!["FalloutNV.esm".to_string()],
    )?;
    let cell_id = s
        .index
        .cells
        .values()
        .find(|c| c.edid.as_deref() == Some(cell.as_str()))
        .map(|c| c.form_id)
        .expect("cell");
    s.open_cell(cell_id);

    // Warm the cache, then time what a frame actually costs.
    let n = s.cell_refs().len();
    let t = Instant::now();
    for _ in 0..frames {
        std::hint::black_box(s.cell_refs().len());
    }
    let cached = t.elapsed() / frames as u32;

    // The cost the cache replaced: rebuilding from the records every frame.
    let t = Instant::now();
    for _ in 0..frames {
        std::hint::black_box(s.build_cell_refs_pub(cell_id).len());
    }
    let rebuilt = t.elapsed() / frames as u32;

    // The lookup the rebuild uses: the old way walked every record in the
    // plugin, once per reference.
    let fids: Vec<u32> = s.cell_refs().iter().map(|r| r.form_id).collect();
    let t = Instant::now();
    for fid in &fids {
        std::hint::black_box(nvre_edit::document::record_of(s.doc.plugin(), *fid));
    }
    let old_lookup = t.elapsed() / n as u32;
    let t = Instant::now();
    for fid in &fids {
        std::hint::black_box(s.doc.record(*fid));
    }
    let new_lookup = t.elapsed() / n as u32;

    println!("cell {cell} ({cell_id:08X}): {n} refs");
    println!("  cached read   {cached:>12?}");
    println!(
        "  rebuild       {rebuilt:>12?}   ({:.0}x)",
        rebuilt.as_secs_f64() / cached.as_secs_f64()
    );
    println!("  lookup, walk  {old_lookup:>12?}   per reference");
    println!("  lookup, index {new_lookup:>12?}   per reference");
    println!(
        "  frame cost, old vs new: {:?} vs {:?}",
        rebuilt + old_lookup * n as u32,
        cached
    );
    Ok(())
}
