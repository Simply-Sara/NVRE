//! How many distinct UVs does a mesh's first UV set hold? A shape whose
//! vertices all sample one texel cannot show a texture even when it has one.
//!
//! Usage: `nifuv <data dir> <schema dir[:layers]> <mesh path substring>`
use nvre_nif::scene::{self, Decoded};
use nvre_nif::Schema;
use std::collections::BTreeSet;
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
        for o in scene::scene_objects(&f, &dec) {
            if o.is_node {
                continue;
            }
            let Some(Ok(block)) = dec.blocks.get(o.block) else {
                continue;
            };
            let Some(d) = block.ref_at("data").and_then(|r| dec.get(r)) else {
                continue;
            };
            let md = scene::mesh_data(d, f.header.bs_version());
            let Some(uv) = md.uv_sets.first() else {
                println!("{:?}: no UVs", o.name);
                continue;
            };
            let set: BTreeSet<(u32, u32)> = uv
                .iter()
                .map(|c| (c[0].to_bits(), c[1].to_bits()))
                .collect();
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for c in uv {
                lo[0] = lo[0].min(c[0]);
                lo[1] = lo[1].min(c[1]);
                hi[0] = hi[0].max(c[0]);
                hi[1] = hi[1].max(c[1]);
            }
            println!(
                "{:?}: {} verts, {} distinct UVs, u {:.4}..{:.4} v {:.4}..{:.4}",
                o.name,
                uv.len(),
                set.len(),
                lo[0],
                hi[0],
                lo[1],
                hi[1]
            );
        }
    }
}
