//! Survey `shader_flags_2` bit histogram (S4-10) and `TexDesc` texture transforms (S4-12)
//! across all 5 Bethesda game corpora.
//!
//! Usage: `flags2_survey [game_id]`

use nvre_editor::game::ALL;
use nvre_nif::scene::Decoded;
use std::collections::BTreeMap;

fn main() {
    let target = std::env::args().nth(1);

    for game in ALL {
        if let Some(ref tg) = target {
            if game.id != tg {
                continue;
            }
        }

        println!("============================================================");
        println!("Game: {}", game.id);
        println!("============================================================");

        let Some(data_dir) = game.data_dir() else {
            println!("  [Data dir not found]");
            continue;
        };
        let Ok(data) = game.open_asset_data(&data_dir) else {
            println!("  [Could not open asset data]");
            continue;
        };

        let mut sf2_hist: BTreeMap<u32, usize> = BTreeMap::new();
        let mut total_sf2_blocks = 0;
        let mut total_tex_transforms = 0;
        let mut total_tex_desc = 0;
        let mut files_scanned = 0;

        let mut count = 0;
        for p in data.vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
            count += 1;
            if count > 1000 {
                break;
            }
            let Some(bytes) = data.read(&p) else { continue };
            let Ok(file) = nvre_nif::NifFile::parse(&bytes, Some(&data.schema)) else {
                continue;
            };
            files_scanned += 1;
            let dec = Decoded::new(&file, &bytes, &data.schema);

            for (id, _) in file.blocks.iter().enumerate() {
                let Some(b) = dec.get(id as i32) else {
                    continue;
                };

                // 1. Check shader_flags_2
                if let Some(f2) = b.int("shader_flags_2") {
                    total_sf2_blocks += 1;
                    for bit in 0..32 {
                        if (f2 & (1 << bit)) != 0 {
                            *sf2_hist.entry(bit).or_default() += 1;
                        }
                    }
                }

                // 2. Check TexDesc texture transforms (e.g. in NiTexturingProperty slots)
                for slot in &[
                    "base_texture",
                    "dark_texture",
                    "detail_texture",
                    "gloss_texture",
                    "glow_texture",
                    "bump_map_texture",
                    "decal_0_texture",
                ] {
                    if let Some(sub) = b.get(slot).and_then(|v| v.as_struct()) {
                        total_tex_desc += 1;
                        if let Some(true) =
                            sub.get("has_texture_transform").and_then(|v| v.as_bool())
                        {
                            total_tex_transforms += 1;
                        }
                    }
                }
            }
        }

        println!("Files scanned: {files_scanned}");
        println!("Total blocks with shader_flags_2: {total_sf2_blocks}");
        if total_sf2_blocks > 0 {
            println!("shader_flags_2 bit histogram:");
            for (bit, count) in &sf2_hist {
                let pct = (*count as f64 / total_sf2_blocks as f64) * 100.0;
                println!("  bit {:>2}: {:>6} occurrences ({:.1}%)", bit, count, pct);
            }
        }

        println!("Total TexDesc slots inspected: {total_tex_desc}");
        println!("Total TexDesc with has_texture_transform=true: {total_tex_transforms}");
        println!();
    }
}
