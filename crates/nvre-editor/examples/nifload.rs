//! Does the editor's own asset path load a game's meshes?
//!
//! The probe measures the schema against raw files. This asks the question
//! the editor actually asks: given a game, can its own VFS and NIF schema
//! read a mesh and give back a scene?
use nvre_nif::scene;
use nvre_editor::game::ALL;

fn main() {
    let game_id = std::env::args().nth(1).expect("a game id");
    let game = ALL
        .iter()
        .find(|g| g.id == game_id)
        .unwrap_or_else(|| panic!("no such game: {game_id}"));
    let data_dir = game
        .data_dir()
        .unwrap_or_else(|| panic!("{}: no data folder", game.id));
    let data = game
        .open_asset_data(&data_dir)
        .unwrap_or_else(|e| panic!("{}: {e}", game.id));
    let (mut nifs, mut parsed, mut blocks, mut objects) = (0u64, 0u64, 0u64, 0u64);
    for p in data.vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        let Some(bytes) = data.read(&p) else { continue };
        nifs += 1;
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&data.schema)) else {
            continue;
        };
        parsed += 1;
        blocks += f.blocks.len() as u64;
        let dec = scene::Decoded::new(&f, &bytes, &data.schema);
        objects += scene::scene_objects(&f, &dec).len() as u64;
        if nifs >= 300 {
            break;
        }
    }
    println!(
        "{game_id}: {nifs} meshes read, {parsed} parsed, {blocks} blocks, {objects} scene objects"
    );
}
