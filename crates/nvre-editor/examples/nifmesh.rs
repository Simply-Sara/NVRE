//! Does a game's mesh actually build, and with how many vertices?
use nvre_editor::game::ALL;
fn main() {
    let game_id = std::env::args().nth(1).expect("a game id");
    let limit: usize = std::env::var("NIF_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let game = ALL.iter().find(|g| g.id == game_id).expect("a game");
    let data_dir = game.data_dir().expect("a data folder");
    let data = game.open_asset_data(&data_dir).expect("the asset data");
    let (mut meshes, mut verts, mut tris) = (0u64, 0u64, 0u64);
    for p in data.vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        let Some(bytes) = data.read(&p) else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&data.schema)) else {
            continue;
        };
        let dec = nvre_nif::scene::Decoded::new(&f, &bytes, &data.schema);
        for (i, b) in f.blocks.iter().enumerate() {
            if !nvre_nif::scene::SHAPE_DATA_TYPES.contains(&b.type_name.as_str()) {
                continue;
            }
            let Some(s) = dec.get(i as i32) else { continue };
            let md = nvre_nif::scene::mesh_data(s, f.header.bs_version());
            if md.positions.is_empty() || md.triangles.is_empty() {
                continue;
            }
            meshes += 1;
            verts += md.positions.len() as u64;
            tris += md.triangles.len() as u64;
            if meshes >= limit as u64 {
                break;
            }
        }
        if meshes >= limit as u64 {
            break;
        }
    }
    println!("{game_id}: {meshes} meshes with geometry, {verts} vertices, {tris} triangles");
}
