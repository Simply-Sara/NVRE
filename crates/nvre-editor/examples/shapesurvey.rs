//! How many shapes of a game's meshes build a drawable mesh?
//!
//! This walks the corpus the way the render path does — through
//! , which is what  calls — so a shape the
//! pipeline silently drops is a shape that is not drawn.
//!
//! Usage: 
use nvre_assets::shape::build_mesh;
use nvre_editor::game::ALL;
use nvre_nif::scene::{scene_objects, Decoded, SHAPE_TYPES};
use std::collections::BTreeMap;

fn main() {
    let game_id = std::env::args().nth(1).expect("a game id");
    let limit: usize = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);
    let game = ALL.iter().find(|g| g.id == game_id).expect("a game");
    let data_dir = game.data_dir().expect("a data folder");
    let data = game.open_asset_data(&data_dir).expect("the asset data");

    let (mut files, mut shapes, mut built, mut empty) = (0u64, 0u64, 0u64, 0u64);
    let mut by_type: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for p in data.vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        if files as usize >= limit {
            break;
        }
        let Some(bytes) = data.read(&p) else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&data.schema)) else { continue };
        files += 1;
        let dec = Decoded::new(&f, &bytes, &data.schema);
        for o in scene_objects(&f, &dec) {
            if o.is_node || !SHAPE_TYPES.contains(&o.type_name.as_str()) {
                continue;
            }
            let Some(shape) = dec.blocks.get(o.block).and_then(|s| s.as_ref().ok()) else { continue };
            let data_block = if let Some(r) = shape.ref_at("data") {
                dec.get(r)
            } else {
                // The Special Edition's and Fallout 4's shape carries its
                // own geometry.
                Some(shape)
            };
            let Some(db) = data_block else { continue };
            shapes += 1;
            let e = by_type.entry(o.type_name.clone()).or_insert((0, 0));
            if build_mesh(db, f.header.bs_version()).is_some() {
                built += 1;
                e.0 += 1;
            } else {
                empty += 1;
                e.1 += 1;
            }
        }
    }
    println!("{game_id}: {files} files, {shapes} shapes");
    println!("  build a mesh: {built}");
    println!("  no geometry:   {empty}");
    for (k, (ok, bad)) in by_type.iter() {
        println!("    {k:28} {ok:7} built, {bad:7} empty");
    }
}
