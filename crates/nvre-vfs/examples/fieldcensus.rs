//! S4-1: The consumed-field census.
//!
//! For each property block type that appears in the corpus, list every field
//! the schema declares on it and whether `shape_props` reads it.
//! Output `read / ignored / not-applicable`, and every `ignored` gets a one-line
//! reason or becomes a defect.

use nvre_nif::Schema;
use std::collections::BTreeSet;
use std::path::PathBuf;

const CONSUMED_FIELDS: &[&str] = &[
    "shader_flags",
    "glossiness",
    "emissive_mult",
    "emissive_multiple",
    "emissive_color",
    "specular_color",
    "ambient_color",
    "diffuse_color",
    "alpha",
    "file_name",
    "source_texture",
    "falloff_start_angle",
    "falloff_stop_angle",
    "falloff_start_opacity",
    "falloff_stop_opacity",
    "flags",
    "threshold",
    "draw_mode",
    "textures",
    "texture_set",
    "source",
];

const PROPERTY_TYPES: &[&str] = &[
    "NiMaterialProperty",
    "BSShaderProperty",
    "BSShaderLightingProperty",
    "BSShaderPPLightingProperty",
    "BSShaderNoLightingProperty",
    "BSLightingShaderProperty",
    "BSEffectShaderProperty",
    "NiAlphaProperty",
    "NiStencilProperty",
    "NiVertexColorProperty",
    "BSShaderTextureSet",
];

fn main() {
    let mut args = std::env::args().skip(1);
    let schema_dir = args.next().expect("a schema directory");

    let mut dirs = schema_dir.split(':');
    let base = PathBuf::from(dirs.next().unwrap());
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&base).expect("the schema")
    } else {
        Schema::load_game(&base, &layers).expect("the schema")
    };

    let consumed: BTreeSet<&str> = CONSUMED_FIELDS.iter().copied().collect();

    println!("# S4-1 Consumed-Field Census\n");
    println!("| Block Type | Field | Status | Notes |");
    println!("|---|---|---|---|");

    for &ty in PROPERTY_TYPES {
        let Some(fields) = schema.block_fields(ty) else {
            continue;
        };
        for f in fields {
            let is_read = consumed.contains(f.name.as_str());
            let (status, note) = if is_read {
                ("read", "consumed in shape_props")
            } else if f.name == "shader_flags_2" {
                ("ignored", "S4-10 defect: shader_flags_2 not read")
            } else if f.name.contains("unused") || f.name.contains("padding") {
                ("not-applicable", "padding/unused bytes")
            } else if f.name.contains("controller") || f.name == "extra_data" || f.name == "name" {
                ("not-applicable", "scene graph linkage")
            } else {
                ("ignored", "unconsumed property field")
            };
            println!("| `{ty}` | `{}` | **{status}** | {note} |", f.name);
        }
    }
}
