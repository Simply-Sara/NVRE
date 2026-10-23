//! Does Fallout 4's data folder read through the editor's own VFS?
//!
//! The question the editor actually asks: given a game's data folder, can its
//! VFS list the meshes, read one, and parse it into a scene with geometry?
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
    println!("{}: {}", game.id, data_dir.display());
    let data = match game.open_asset_data(&data_dir) {
        Ok(d) => d,
        Err(e) => {
            println!("  cannot open the assets: {e}");
            return;
        }
    };
    let nifs = data.vfs.paths(|p| p.to_lowercase().ends_with(".nif"));
    let dds = data.vfs.paths(|p| p.to_lowercase().ends_with(".dds"));
    println!(
        "  {} archives, {} .nif paths, {} .dds paths",
        data.vfs.archives().count(),
        nifs.len(),
        dds.len()
    );
    let (mut read, mut parsed, mut shapes, mut drawn) = (0usize, 0usize, 0usize, 0usize);
    for p in nifs.iter().take(200) {
        let Some(Ok(bytes)) = data.vfs.read(p) else {
            continue;
        };
        read += 1;
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&data.schema)) else {
            continue;
        };
        parsed += 1;
        let dec = nvre_nif::scene::Decoded::new(&f, &bytes, &data.schema);
        let objects = nvre_nif::scene::scene_objects(&f, &dec);
        for o in objects.iter().filter(|o| !o.is_node) {
            shapes += 1;
            let Some(shape) = dec.blocks.get(o.block).and_then(|s| s.as_ref().ok()) else {
                continue;
            };
            let db = if let Some(r) = shape.ref_at("data") {
                dec.get(r)
            } else if o.type_name == "BSTriShape" {
                Some(shape)
            } else {
                None
            };
            let Some(db) = db else { continue };
            let md = nvre_nif::scene::mesh_data(db, f.header.bs_version());
            if !md.positions.is_empty() && !md.triangles.is_empty() {
                drawn += 1;
            }
        }
    }
    println!(
        "  first 200 .nif: {read} read, {parsed} parsed, {shapes} shapes, {drawn} drawable"
    );
    // One texture, read all the way through.
    if let Some(t) = dds.first() {
        match data.vfs.read(t) {
            Some(Ok(b)) => println!("  texture {}: {} bytes", t, b.len()),
            Some(Err(e)) => println!("  texture {t}: FAILED {e}"),
            None => println!("  texture {t}: not found"),
        }
    }
}
