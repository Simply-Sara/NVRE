//! The consumed-field census (S4-1).
//!
//! For each game, for each property block type that appears in the corpus,
//! list every field the schema declares on it and whether `shape_props`
//! reads it.
//!
//! Output: `read / ignored / not-applicable`. Every `ignored` field is either
//! documented or identified as a defect.
//!
//! Usage: `fieldcensus [game_id]`

use nvre_editor::game::ALL;
use nvre_nif::scene::{scene_objects, Decoded};
use std::collections::{BTreeMap, BTreeSet};

/// The fields accessed in `shape_props` and its helper functions
/// (`texture_set`, `tex_desc`, `alpha_mode`, `blend_cut`, etc.).
const READ_FIELDS: &[&str] = &[
    "properties",
    "shader_property",
    "alpha_property",
    "shader_flags",
    "shader_flags_1",
    "glossiness",
    "smoothness",
    "emissive_color",
    "emissive_mult",
    "emissive_multiple",
    "alpha",
    "texture_set",
    "file_name",
    "source_texture",
    "falloff_start_angle",
    "falloff_stop_angle",
    "falloff_start_opacity",
    "falloff_stop_opacity",
    "base_texture",
    "glow_texture",
    "normal_texture",
    "flags",
    "threshold",
    "draw_mode",
    "num_textures",
    "textures",
    "source",
];

/// Fields that are structurally required by the NIF container format or
/// header/inheritance conventions, not visual material properties.
const STRUCTURAL_FIELDS: &[&str] = &[
    "name",
    "extra_data_list",
    "num_extra_data_list",
    "controller",
    "time_controller",
    "target",
];

fn main() {
    let target_game = std::env::args().nth(1);

    for game in ALL {
        if let Some(ref tg) = target_game {
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

        // 1. Collect all property block types from the schema that inherit from
        // NiProperty or BSShaderProperty, or appear in the block definitions.
        let mut seen_property_types = BTreeSet::new();

        for block_name in data.schema.blocks.keys() {
            if data.schema.is_a(block_name, "NiProperty")
                || data.schema.is_a(block_name, "BSShaderProperty")
                || block_name.ends_with("Property")
            {
                seen_property_types.insert(block_name.clone());
            }
        }

        println!(
            "Schema property types evaluated: {}",
            seen_property_types.len()
        );

        let mut read_count = 0;
        let mut ignored_count = 0;
        let mut structural_count = 0;

        let mut ignored_by_type: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for prop_type in &seen_property_types {
            let fields = data.schema.block_fields(prop_type).unwrap_or_default();
            println!("\nProperty: {prop_type} ({} fields)", fields.len());
            for f in &fields {
                let status = if READ_FIELDS.contains(&f.name.as_str()) {
                    read_count += 1;
                    "READ"
                } else if STRUCTURAL_FIELDS.contains(&f.name.as_str()) {
                    structural_count += 1;
                    "STRUCTURAL (n/a)"
                } else {
                    ignored_count += 1;
                    ignored_by_type
                        .entry(prop_type.clone())
                        .or_default()
                        .push(f.name.clone());
                    "IGNORED"
                };
                println!("  - {:<30} [{}]", f.name, status);
            }
        }

        println!("\n--- Summary for {} ---", game.id);
        println!("  READ fields:       {read_count}");
        println!("  STRUCTURAL fields: {structural_count}");
        println!("  IGNORED fields:    {ignored_count}");

        if !ignored_by_type.is_empty() {
            println!("\n  Ignored fields backlog by property:");
            for (ty, f_list) in &ignored_by_type {
                println!("    {ty}:");
                for f in f_list {
                    println!("      * {f}");
                }
            }
        }
        println!();
    }
}
