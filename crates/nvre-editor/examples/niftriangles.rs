//! How many triangles of a game's meshes name a vertex that is not there?
//!
//! S3-1. A triangle whose indices are outside the shape's vertex count is
//! dropped rather than drawn, because a stray triangle appears where the
//! model should be empty. Dropping it is right; swallowing the count is
//! not, because a shape that drops one has a data block and a vertex count
//! that disagree, and that is a defect in the reading rather than in the
//! file.
//!
//! The count is over the whole corpus of a game, and the shapes that drop
//! triangles are counted too, so one shape that drops many stands out from
//! many shapes that drop one.
//!
//! Usage: `niftriangles <game id> [limit files]`

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

    let (mut files, mut shapes, mut dropped_shapes, mut dropped) = (0u64, 0u64, 0u64, 0u64);
    // Which types drop triangles, so a type that always drops is visible.
    let mut by_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut worst: Vec<(u64, String)> = Vec::new();

    for p in data.vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        if files as usize >= limit {
            break;
        }
        let Some(bytes) = data.read(&p) else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&data.schema)) else {
            continue;
        };
        files += 1;
        let dec = Decoded::new(&f, &bytes, &data.schema);
        let objects = scene_objects(&f, &dec);
        for (i, o) in objects.iter().enumerate() {
            if o.is_node || !SHAPE_TYPES.contains(&o.type_name.as_str()) {
                continue;
            }
            let Some(shape) = dec.get(i as i32) else {
                continue;
            };
            let data_block = if let Some(r) = shape.ref_at("data") {
                dec.get(r)
            } else if o.type_name == "BSTriShape" {
                Some(shape)
            } else {
                None
            };
            let Some(data_block) = data_block else {
                continue;
            };
            let Some((_mesh, _tris, n)) = build_mesh(data_block, f.header.bs_version()) else {
                continue;
            };
            shapes += 1;
            if n > 0 {
                dropped_shapes += 1;
                dropped += n as u64;
                *by_type.entry(o.type_name.clone()).or_insert(0) += 1;
                worst.push((n as u64, format!("{} {}", o.type_name, p)));
            }
        }
    }

    println!(
        "{game_id}: {files} files, {shapes} shapes with geometry, \
         {dropped_shapes} of them drop a triangle ({dropped} triangles)",
    );
    if !by_type.is_empty() {
        println!("  shapes that drop triangles, by type:");
        for (k, v) in by_type.iter().rev() {
            println!("    {v:8} {k}");
        }
        worst.sort_by(|a, b| b.0.cmp(&a.0));
        println!("  the worst:");
        for (n, what) in worst.iter().take(5) {
            println!("    {n:6} {what}");
        }
    }
}
