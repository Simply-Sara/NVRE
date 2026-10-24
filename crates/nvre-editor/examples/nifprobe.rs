//! What does NVRE make of one mesh? Dump every block, field by field, and the
//! ShapeProps the editor builds from each shape — so a mesh that draws wrong
//! can be traced back to the fields that decide it.
//!
//! Usage: `nifprobe <data dir> <schema dir[:layers]> <mesh path substring>`
use nvre_assets::shape::{shape_props, ShapeProps};
use nvre_nif::scene::{self, Decoded};
use nvre_nif::value::Value;
use nvre_nif::Schema;
use std::collections::HashMap;
use std::path::PathBuf;

fn dump_value(v: &Value, indent: usize) -> String {
    let pad = " ".repeat(indent);
    match v {
        Value::Struct(s) => {
            let mut out = String::from("{\n");
            for (k, f) in &s.fields {
                out.push_str(&format!("{pad}  {k}: {}\n", dump_value(f, indent + 2)));
            }
            out.push_str(&format!("{pad}}}"));
            out
        }
        Value::List(l) => {
            if l.len() > 6 {
                format!("[{} items: {:?} ...]", l.len(), &l[..3])
            } else {
                format!("{l:?}")
            }
        }
        other => format!("{other:?}"),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("a data folder");
    let schema_dir = args.next().expect("a schema directory");
    let want = args.next().expect("a mesh path substring").to_lowercase();
    let all = std::env::var("NIF_ALL").is_ok();

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

    let paths: Vec<String> =
        vfs.paths(|p| p.to_lowercase().ends_with(".nif") && p.to_lowercase().contains(&want));
    println!("{} candidate files for {want:?}", paths.len());
    for p in paths.iter().take(5) {
        println!("  {p}");
    }

    for p in paths.iter().take(3) {
        let Some(Ok(bytes)) = vfs.read(p.as_str()) else {
            continue;
        };
        println!("=== {p} ({} bytes)", bytes.len());
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&schema)) else {
            println!("  PARSE FAILED");
            continue;
        };
        println!(
            "  version {:#010x} user_version {} bs_version {} blocks {}",
            f.header.version,
            f.header.user_version,
            f.header.bs_version(),
            f.blocks.len()
        );
        let dec = Decoded::new(&f, &bytes, &schema);
        let objects = scene::scene_objects(&f, &dec);
        let by_block: HashMap<usize, &scene::SceneObject> =
            objects.iter().map(|o| (o.block, o)).collect();

        // Every block, in order.
        for (i, b) in f.blocks.iter().enumerate() {
            match dec.blocks.get(i) {
                Some(Ok(s)) => {
                    println!("block {i} {}", b.type_name);
                    for (k, v) in &s.fields {
                        if !all
                            && matches!(
                                k.as_str(),
                                "num_extra_data"
                                    | "extra_data"
                                    | "rotation"
                                    | "translation"
                                    | "bounds"
                            )
                        {
                            continue;
                        }
                        let text = dump_value(v, 6);
                        if text.len() > 400 {
                            println!("      {k} = {}...", &text[..400]);
                        } else {
                            println!("      {k} = {text}");
                        }
                    }
                }
                Some(Err(e)) => println!("block {i} {} DECODE FAILED: {e:?}", b.type_name),
                None => {}
            }
        }

        // What the renderer makes of each shape.
        for o in &objects {
            if o.is_node {
                continue;
            }
            let props: ShapeProps = shape_props(&dec, o, &by_block);
            let Some(Ok(block)) = dec.blocks.get(o.block) else {
                continue;
            };
            let data = block
                .ref_at("data")
                .and_then(|r| dec.get(r))
                .or_else(|| dec.get(o.block as i32));
            let (verts, tris, colors, strips) = match data {
                Some(d) => {
                    let md = scene::mesh_data(d, f.header.bs_version());
                    (
                        md.positions.len(),
                        md.triangles.len(),
                        md.colors.len(),
                        d.list("points").map_or(0, |l| l.len()),
                    )
                }
                None => (0, 0, 0, 0),
            };
            println!(
                "SHAPE block {} {} name={:?} verts={verts} tris={tris} strips={strips} vertex_colors={colors}",
                o.block, o.type_name, o.name
            );
            println!(
                "  texture={:?} normal={:?} glow={:?} alpha={:?} emissive={:?} gloss={:?} unlit={} double_sided={} alpha_mode={:?}",
                props.texture,
                props.normal_texture,
                props.glow,
                props.alpha,
                props.emissive,
                props.glossiness,
                props.unlit,
                props.double_sided,
                props.alpha_mode
            );
            if let Some(d) = data {
                if let Some(b) = d.get("bounds") {
                    println!("  bounds = {}", dump_value(b, 2));
                }
            }
        }
    }
}
