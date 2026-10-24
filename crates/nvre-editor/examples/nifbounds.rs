//! The world-space bounds of every shape in one mesh, and what the renderer
//! would draw it with — the size of the thing on screen, next to the reason
//! it looks the way it does.
//!
//! Usage: `nifbounds <data dir> <schema dir[:layers]> <mesh path substring>`
use nvre_assets::shape::{shape_props, ShapeProps};
use nvre_nif::scene::{self, Decoded, Transform};
use nvre_nif::Schema;
use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("a data folder");
    let schema_dir = args.next().expect("a schema directory");
    let want = args.next().expect("a mesh path substring").to_lowercase();

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

    for p in vfs
        .paths(|p| p.to_lowercase().ends_with(".nif") && p.to_lowercase().contains(&want))
        .iter()
        .take(1)
    {
        let Some(Ok(bytes)) = vfs.read(p.as_str()) else {
            continue;
        };
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&schema)) else {
            continue;
        };
        println!("=== {p}");
        let dec = Decoded::new(&f, &bytes, &schema);
        let objects = scene::scene_objects(&f, &dec);
        let by_block: HashMap<usize, &scene::SceneObject> =
            objects.iter().map(|o| (o.block, o)).collect();
        let by_block_map: HashMap<usize, usize> = objects
            .iter()
            .enumerate()
            .map(|(i, o)| (o.block, i))
            .collect();
        for o in &objects {
            if o.is_node {
                continue;
            }
            let Some(Ok(block)) = dec.blocks.get(o.block) else {
                continue;
            };
            let props: ShapeProps = shape_props(&dec, o, &by_block);
            // The shape's world transform, walking the parents up.
            let mut world = Transform::IDENTITY;
            let mut cur: Option<usize> = Some(o.block);
            let mut depth = 0;
            while let Some(b) = cur {
                if let Some(s) = dec.get(b as i32) {
                    if let Some(t) = Transform::from_struct(s) {
                        // Translate then scale, approximately: scale is 1.
                        for k in 0..3 {
                            world.translation[k] += t.translation[k] * world.scale;
                        }
                        world.scale *= t.scale;
                    }
                }
                let idx = by_block_map.get(&b).copied();
                cur = idx.and_then(|i| objects[i].parent);
                depth += 1;
                if depth > 32 {
                    break;
                }
            }
            let data = block
                .ref_at("data")
                .and_then(|r| dec.get(r))
                .or_else(|| dec.get(o.block as i32));
            let Some(d) = data else { continue };
            let md = scene::mesh_data(d, f.header.bs_version());
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &md.positions {
                for k in 0..3 {
                    let x = v[k] * world.scale + world.translation[k];
                    lo[k] = lo[k].min(x);
                    hi[k] = hi[k].max(x);
                }
            }
            println!(
                "{} {:?}: {} verts, x {:.0}..{:.0}  y {:.0}..{:.0}  z {:.0}..{:.0}  scale {:.3}",
                o.type_name,
                o.name,
                md.positions.len(),
                lo[0],
                hi[0],
                lo[1],
                hi[1],
                lo[2],
                hi[2],
                world.scale
            );
            println!(
                "   drawn with: texture={:?} unlit={} alpha={:?} double_sided={} emissive={:?}",
                props.texture, props.unlit, props.alpha_mode, props.double_sided, props.emissive
            );
        }
    }
}
