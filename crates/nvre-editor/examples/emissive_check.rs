//! Do the emissive fields reach `ShapeProps`, per game?
//!
//! Usage: `emissive_check <game id> <data dir> [mesh substring]`
//!
//! Prints, per shape, the emissive colour, its multiplier and the glow map as
//! `shape_props` reads them. `shape_props` is the whole of what the material
//! build consumes — the model layer reads no property directly — so what this
//! shows is what will reach the material.
//!
//! The two spellings matter: `emissive_mult` on `NiMaterialProperty` (FNV and
//! FO3) and `emissive_multiple` on `BSLightingShaderProperty` (Skyrim). A game
//! that reports `x1.000` with a real colour is fine; one that reports a
//! multiplier of 1.0 where the file says otherwise is the bug this finds.
use nvre_assets::shape::shape_props;
use nvre_editor::game::GameProfile;
use nvre_nif::scene::{self, Decoded};
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let game_id = args
        .next()
        .expect("a game id (fnv, fo3, skyrim, skyrimse, fo4)");
    let data_dir = args.next().expect("a data folder");
    let want = args.next().unwrap_or_default().to_lowercase();

    let profile = GameProfile::by_id(&game_id).expect("a known game");
    println!("game {}", profile.name);
    let assets = profile
        .open_asset_data(PathBuf::from(&data_dir).as_path())
        .expect("the game's assets");
    let schema = profile.load_nif_schema().expect("the game's schema");

    for p in assets
        .vfs
        .paths(|p| {
            p.to_lowercase().ends_with(".nif")
                && (want.is_empty() || p.to_lowercase().contains(&want))
        })
        .iter()
        .take(1)
    {
        let Some(Ok(bytes)) = assets.vfs.read(p.as_str()) else {
            continue;
        };
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&schema)) else {
            println!("{p}: PARSE FAILED");
            continue;
        };
        println!("\n=== {p} bs_version {}", f.header.bs_version());
        let dec = Decoded::new(&f, &bytes, &schema);
        let objects = scene::scene_objects(&f, &dec);
        let mut shapes = 0usize;
        for o in &objects {
            if o.is_node {
                continue;
            }
            shapes += 1;
            let props = shape_props(&dec, o, &Default::default());
            println!(
                "  shape {:<4} {:<16} emissive={:<22} x{:<7.3} glow={:?} unlit={}",
                o.block,
                o.type_name,
                props
                    .emissive
                    .map(|e| format!("({:.2},{:.2},{:.2})", e[0], e[1], e[2]))
                    .unwrap_or_else(|| "-".to_string()),
                props.emissive_mult,
                props.glow.as_deref(),
                props.unlit,
            );
        }
        println!("  {shapes} shapes");
    }
}
