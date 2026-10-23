//! How many of a game's shapes end up with no texture, and why?
//!
//! For every shape in every mesh: does `shape_props` find a diffuse texture?
//! If not, does the shape's `NiTexturingProperty` name one (the Fallout way),
//! which `shape_props` never reads?
//!
//! Usage: `texturesurvey <data dir> <schema dir[:layers]>`
use nvre_assets::shape::{shape_props, ShapeProps};
use nvre_nif::scene::{self, Decoded};
use nvre_nif::Schema;
use std::collections::HashMap;
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

    let (mut files, mut shapes, mut textured, mut untextured) = (0u64, 0u64, 0u64, 0u64);
    // Untextured shapes that name a diffuse texture somewhere shape_props
    // does not look.
    let mut via_texturing_property = 0u64;
    let mut via_no_lighting_file_name = 0u64;
    let mut nowhere = 0u64;
    // Alpha blend flags the shape's NiAlphaProperty asks for but the
    // material does not use.
    let mut blend_asked = 0u64;
    let mut blend_ignored = 0u64;
    let mut examples: Vec<String> = Vec::new();

    // NIF_FILTER narrows the survey to paths holding a substring.
    let filter = std::env::var("NIF_FILTER").unwrap_or_default().to_lowercase();
    for p in vfs.paths(|p| p.to_lowercase().ends_with(".nif") && p.to_lowercase().contains(&filter)) {
        let Some(Ok(bytes)) = vfs.read(p.as_str()) else {
            continue;
        };
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&schema)) else {
            continue;
        };
        files += 1;
        let dec = Decoded::new(&f, &bytes, &schema);
        let objects = scene::scene_objects(&f, &dec);
        let by_block: HashMap<usize, &scene::SceneObject> =
            objects.iter().map(|o| (o.block, o)).collect();
        for o in &objects {
            if o.is_node {
                continue;
            }
            let Some(Ok(block)) = dec.blocks.get(o.block) else {
                continue;
            };
            shapes += 1;
            let props: ShapeProps = shape_props(&dec, o, &by_block);
            let mut base_tex: Option<String> = None;
            let mut no_lighting: Option<String> = None;
            let mut alpha_flags: Option<i64> = None;
            let mut refs: Vec<i32> = block.refs("properties").unwrap_or_default();
            if let Some(x) = block.ref_at("shader_property") {
                refs.push(x);
            }
            if let Some(x) = block.ref_at("alpha_property") {
                refs.push(x);
            }
            for r in refs {
                let Some(prop) = dec.get(r) else { continue };
                if prop.get("base_texture").is_some() {
                    if let Some(src) = prop
                        .get("base_texture")
                        .and_then(|b| b.as_struct())
                        .and_then(|b| b.get("source"))
                        .and_then(|v| v.as_ref())
                    {
                        if let Some(t) = dec.get(src) {
                            base_tex = t.text("file_name").map(str::to_string);
                        }
                    }
                }
                if prop.get("shader_type").is_some() {
                    if let Some(n) = prop.text("file_name").filter(|s| !s.is_empty()) {
                        no_lighting = Some(n.to_string());
                    }
                }
                if prop.get("stencil_ref").is_none() {
                    if let Some(fl) = prop.int("flags") {
                        if prop.get("threshold").is_some() {
                            alpha_flags = Some(fl);
                        }
                    }
                }
            }
            if props.texture.is_some() {
                textured += 1;
            } else {
                untextured += 1;
                if base_tex.is_some() {
                    via_texturing_property += 1;
                } else if no_lighting.is_some() {
                    via_no_lighting_file_name += 1;
                } else {
                    nowhere += 1;
                }
                if examples.len() < 12 && !nvre_assets::model::is_editor_marker(o.name.as_deref()) {
                    examples.push(format!(
                        "{p} :: {} base_tex={base_tex:?} no_lighting={no_lighting:?}",
                        o.name.clone().unwrap_or_default()
                    ));
                }
            }
            if let Some(fl) = alpha_flags {
                if fl & 1 != 0 {
                    blend_asked += 1;
                    // A blend-and-test shape with a high threshold is drawn
                    // alpha-tested on purpose; only an opaque one is a shape
                    // whose blend the material threw away.
                    if props.alpha_mode == bevy::material::AlphaMode::Opaque {
                        blend_ignored += 1;
                    }
                }
            }
        }
    }
    println!("{files} meshes, {shapes} shapes");
    println!("  textured:   {textured}");
    println!("  untextured: {untextured}");
    println!("    texture in NiTexturingProperty.base_texture (not read): {via_texturing_property}");
    println!("    texture in shader property's file_name (not read):      {via_no_lighting_file_name}");
    println!("    no texture anywhere:                                   {nowhere}");
    println!("  alpha blend asked for by NiAlphaProperty: {blend_asked}, ignored: {blend_ignored}");
    for e in examples {
        println!("  {e}");
    }
}
