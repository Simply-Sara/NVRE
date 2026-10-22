//! Trace how one block is read, field by field, so a layout can be checked.
use nvre_nif::Schema;
use std::path::PathBuf;

fn main() {
    let want = std::env::args().nth(1).expect("a block type");
    let data_dir = std::env::args().nth(2).expect("a data folder");
    let schema_dir = std::env::args().nth(3).expect("a schema directory");

    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&PathBuf::from(base)).expect("the schema")
    } else {
        Schema::load_game(&PathBuf::from(base), &layers).expect("the schema")
    };
    let vfs = nvre_vfs::Vfs::open_default(PathBuf::from(&data_dir).as_path()).expect("the archives");

    for p in vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        let Some(r) = vfs.read(p.as_str()) else {
            continue;
        };
        let Ok(data) = r else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
            continue;
        };
        for (i, b) in f.blocks.iter().enumerate() {
            if b.type_name != want {
                continue;
            }
            println!(
                "{want}: {} bytes at {}",
                b.range.end - b.range.start,
                b.range.start
            );
            let (st, _) = f.decode_block_status(i, &schema, &data);
            println!("  status {st:?}");
            let _ = f.decode_block(i, &schema, &data);
            return;
        }
    }
    println!("no {want} found");
}
