//! What block types do a game's meshes hold?
//!
//! The scene reader counts a node or a shape; this counts every block, so a
//! game whose geometry uses a different block name is visible.
use nvre_editor::game::ALL;
use std::collections::BTreeMap;

fn main() {
    let game_id = std::env::args().nth(1).expect("a game id");
    let limit: usize = std::env::var("NIF_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let game = ALL
        .iter()
        .find(|g| g.id == game_id)
        .unwrap_or_else(|| panic!("no such game: {game_id}"));
    let data_dir = game.data_dir().expect("a data folder");
    let data = game.open_asset_data(&data_dir).expect("the asset data");
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut files = 0u64;
    for p in data.vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        let Some(bytes) = data.read(&p) else { continue };
        let Ok(f) = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)) else {
            continue;
        };
        files += 1;
        for b in &f.blocks {
            *counts.entry(b.type_name.clone()).or_insert(0) += 1;
        }
        if files as usize >= limit {
            break;
        }
    }
    println!("{game_id}: {files} files");
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (k, n) in v.iter().take(25) {
        println!("  {n:7} {k}");
    }
}
