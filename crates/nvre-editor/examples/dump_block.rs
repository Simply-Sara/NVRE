//! Dump a block's bytes, so a layout can be checked against the real thing.
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
        for b in &f.blocks {
            if b.type_name != want {
                continue;
            }
            let bytes = &data[b.range.start..b.range.end];
            println!("{want}, {} bytes, from {p}", bytes.len());
            for (i, chunk) in bytes.chunks(16).enumerate() {
                let hex: Vec<String> = chunk.iter().map(|x| format!("{x:02x}")).collect();
                let nums: Vec<String> = chunk
                    .chunks(4)
                    .map(|c| {
                        if c.len() == 4 {
                            format!("{:.3}", f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                        } else {
                            String::new()
                        }
                    })
                    .collect();
                println!("  {:4} {:<48} {}", i * 16, hex.join(" "), nums.join(" "));
            }
            return;
        }
    }
    println!("no {want} found");
}
