//! How often does `resolve_strings` overwrite a numeric field with string
//! table text? Every `Value::Num` whose value is a valid table index is
//! replaced, whatever the field is — so a `scale` of 1.0 becomes "BSX".
//!
//! Usage: `strclobber <data dir> <schema dir[:layers]>`
use nvre_nif::scene::{self, Decoded};
use nvre_nif::value::Value;
use nvre_nif::Schema;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("a data folder");
    let schema_dir = args.next().expect("a schema directory");

    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&PathBuf::from(base)).expect("the schema")
    } else {
        Schema::load_game(&PathBuf::from(base), &layers).expect("the schema")
    };
    let vfs =
        nvre_vfs::Vfs::open_default(PathBuf::from(&data_dir).as_path()).expect("the archives");
    let filter = std::env::var("NIF_FILTER")
        .unwrap_or_default()
        .to_lowercase();

    // Which field names got a string, and how often.
    let mut by_field: BTreeMap<String, u64> = BTreeMap::new();
    let mut objects = 0u64;
    let mut objects_with_str_scale = 0u64;
    let mut files = 0u64;
    let mut transforms_lost = 0u64;

    for p in vfs.paths(|p| p.to_lowercase().ends_with(".nif") && p.to_lowercase().contains(&filter))
    {
        let Some(Ok(bytes)) = vfs.read(p.as_str()) else {
            continue;
        };
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&schema)) else {
            continue;
        };
        files += 1;
        let dec = Decoded::new(&f, &bytes, &schema);
        for (i, b) in dec.blocks.iter().enumerate() {
            let Ok(s) = b else { continue };
            let is_object = dec.is_a(i, "NiAVObject");
            if is_object {
                objects += 1;
            }
            let mut scale_is_str = false;
            for (k, v) in &s.fields {
                if matches!(v, Value::Str(_)) {
                    // A field the schema declares as a string is fine; the
                    // ones below are numeric types that cannot hold text.
                    if matches!(
                        k.as_str(),
                        "scale"
                            | "glossiness"
                            | "alpha"
                            | "threshold"
                            | "frequency"
                            | "num_properties"
                            | "num_strips"
                            | "num_triangles"
                            | "emissive_mult"
                            | "stop_time"
                            | "num_children"
                            | "num_materials"
                            | "texture_count"
                            | "shader_type"
                            | "shader_flags"
                            | "flags"
                            | "integer_data"
                            | "num_keys"
                            | "falloff_start_angle"
                            | "falloff_stop_angle"
                            | "falloff_start_opacity"
                            | "falloff_stop_opacity"
                            | "environment_map_scale"
                            | "refraction_strength"
                            | "group_id"
                            | "keep_flags"
                            | "compress_flags"
                            | "num_vertices"
                            | "num_shader_textures"
                            | "active_material"
                    ) {
                        *by_field.entry(k.clone()).or_insert(0) += 1;
                        if k == "scale" {
                            scale_is_str = true;
                        }
                    }
                }
            }
            if scale_is_str {
                objects_with_str_scale += 1;
                if scene::Transform::from_struct(s).is_none() {
                    transforms_lost += 1;
                }
            }
        }
    }
    println!("{files} meshes, {objects} NiAVObject blocks");
    println!("  blocks whose `scale` decoded to text: {objects_with_str_scale}");
    println!("  blocks whose transform therefore fell back to identity: {transforms_lost}");
    println!("  numeric fields holding string-table text, by field:");
    for (k, n) in &by_field {
        println!("    {n:8} {k}");
    }
}
