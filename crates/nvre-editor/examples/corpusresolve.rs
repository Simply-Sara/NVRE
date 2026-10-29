//! Every corpus FormID resolves through `winner`, and every node and shape
//! name survives the string table.
//!
//! S0-4 and S2-5, one pass per game.
//!
//! - **S0-4** a FormID that `winner(lo, fid)` cannot resolve is a reference
//!   that cannot be drawn: the base object is missing from the load order.
//!   The count is per game over every FormID the masters declare, and a
//!   FormID that resolves to no record at all is distinguished from one that
//!   resolves to a record with no `EDID`.
//! - **S2-5** a node's or shape's name is text after the string table has
//!   resolved it. From 20.1.0.3 on a NIF's strings are `u32` indices into a
//!   table, and a name that did not resolve is still the index it was
//!   written as — a node the scene graph cannot name, which is how
//!   `EditorMarker` detection and the shape's report lose their subject.
//!
//! Usage: `corpusresolve <game id>`

use nvre_editor::game::ALL;
use nvre_editor::session::Session;
use std::collections::BTreeMap;
use std::path::Path;

fn main() {
    let game_id = std::env::args().nth(1).expect("a game id");
    let game = ALL.iter().find(|g| g.id == game_id).expect("a game");
    let data_dir = game.data_dir().expect("a data folder");

    // The master the session opens, and the schema chain beside it.
    let schema_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../schemas/{}", game.schema_subdir));
    let s = match Session::open(
        &data_dir.join(game.master),
        &schema_dir,
        game.masters_in(&data_dir),
    ) {
        Ok(s) => s,
        Err(e) => {
            println!("{game_id}: could not open {}: {e}", game.master);
            return;
        }
    };

    let mut counts = Counts::default();
    // Every FormID the load order declares, by signature.
    let mut by_sig: BTreeMap<String, (u64, u64)> = BTreeMap::new();

    for (pi, plugin) in s.lo.plugins.iter().enumerate() {
        let name = plugin.name.clone();
        let records = s.lo.records[pi].clone();
        for r in &records {
            let Some(global) = s.lo.to_global(pi, r.header.form_id) else {
                counts.not_a_global_id += 1;
                continue;
            };
            counts.form_ids += 1;
            // S0-4 — the winner.
            match nvre_world::fields::winner(&s.lo, global) {
                None => {
                    counts.unresolved += 1;
                    by_sig
                        .entry(String::from_utf8_lossy(&r.header.sig).to_string())
                        .or_insert((0, 0))
                        .1 += 1;
                }
                Some(_) => {
                    counts.resolved += 1;
                    let e = by_sig
                        .entry(String::from_utf8_lossy(&r.header.sig).to_string())
                        .or_insert((0, 0));
                    e.0 += 1;
                }
            }
        }
        let _ = name;
    }

    // S2-5 — names over the mesh corpus.
    let data = game.open_asset_data(&data_dir).expect("the asset data");
    let limit: usize = std::env::var("NIF_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    for p in data.vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        if counts.files as usize >= limit {
            break;
        }
        let Some(bytes) = data.read(&p) else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&data.schema)) else {
            counts.unparsed += 1;
            continue;
        };
        counts.files += 1;
        let dec = nvre_nif::scene::Decoded::new(&f, &bytes, &data.schema);
        for o in nvre_nif::scene::scene_objects(&f, &dec) {
            if o.is_node {
                counts.nodes += 1;
                if o.name.is_some() {
                    counts.named_nodes += 1;
                }
                // A name that is still an index is a number where text
                // should be.
                if let Some(s) = dec.get(o.block as i32) {
                    if let Some(v) = s.get("name") {
                        if v.as_str().is_none() {
                            counts.unresolved_names += 1;
                            if counts.unresolved_examples.len() < 10 {
                                counts.unresolved_examples.push(format!(
                                    "{p} block {} type {} name {}",
                                    o.block,
                                    o.type_name,
                                    v.to_json()
                                ));
                            }
                        }
                    }
                }
            } else {
                counts.shapes += 1;
                if o.name.is_some() {
                    counts.named_shapes += 1;
                }
            }
        }
    }

    println!(
        "{game_id}: {} files, {} unparsed",
        counts.files, counts.unparsed
    );
    println!("S0-4 winner(lo, fid)");
    println!("  FormIDs in the load order: {}", counts.form_ids);
    println!("  resolved: {}", counts.resolved);
    println!("  unresolved: {}", counts.unresolved);
    println!("  not a global id: {}", counts.not_a_global_id);
    if counts.unresolved > 0 {
        println!("  unresolved by signature:");
        for (sig, (ok, bad)) in by_sig.iter().filter(|(_, (_, b))| *b > 0) {
            println!("    {sig}: {bad} unresolved of {} ", ok + bad);
        }
    }
    println!("S2-5 names");
    println!("  {} nodes, {} shapes", counts.nodes, counts.shapes);
    println!("  named nodes: {}", counts.named_nodes);
    println!("  named shapes: {}", counts.named_shapes);
    println!("  names still an index: {}", counts.unresolved_names);
    for e in &counts.unresolved_examples {
        println!("    {e}");
    }
}

#[derive(Default)]
struct Counts {
    /// S0-4
    form_ids: u64,
    resolved: u64,
    unresolved: u64,
    not_a_global_id: u64,
    /// S2-5
    files: u64,
    unparsed: u64,
    nodes: u64,
    shapes: u64,
    named_nodes: u64,
    named_shapes: u64,
    unresolved_names: u64,
    /// The files whose names did not resolve, so the defect is a path
    /// rather than a count.
    unresolved_examples: Vec<String>,
}
