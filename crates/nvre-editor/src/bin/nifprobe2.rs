//! Does NVRE's own NIF reader read a game's meshes?
//!
//! The measure: how many blocks the schema explains, and whether the reading
//! used exactly the bytes each block holds.
use nvre_nif::Schema;
use std::path::PathBuf;

fn main() {
    let data_dir = std::env::args().nth(1).expect("a data folder");
    let schema_dir = std::env::args().nth(2).expect("a schema directory");
    let limit: usize = std::env::var("NIF_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    // The schema directory may be a base plus layers: the first is the base,
    // the rest are layers over it.
    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&PathBuf::from(base)).expect("the schema")
    } else {
        Schema::load_game(&PathBuf::from(base), &layers).expect("the schema")
    };

    // The archive list comes from the command line, so the probe works for a
    // game whose archives are not Fallout's.
    let archives: Vec<String> = std::env::args().skip(3).collect();
    let vfs = if archives.is_empty() {
        fnv_vfs::Vfs::open_default(PathBuf::from(&data_dir).as_path()).expect("the archives")
    } else {
        fnv_vfs::Vfs::open(PathBuf::from(&data_dir).as_path(), &archives).expect("the archives")
    };

    let paths = vfs.paths(|p| p.to_lowercase().ends_with(".nif"));
    println!("the archive holds {} nif files", paths.len());

    let mut files = 0u64;
    let mut parse_fail = 0u64;
    let mut blocks = 0u64;
    let mut unknown = 0u64;
    let mut short = 0u64;
    let mut err = 0u64;
    let mut errors: Vec<String> = Vec::new();

    for p in paths {
        let Some(r) = vfs.read(p.as_str()) else {
            continue;
        };
        let Ok(data) = r else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
            parse_fail += 1;
            continue;
        };
        files += 1;
        for i in 0..f.blocks.len() {
            blocks += 1;
            let (st, _) = f.decode_block_status(i, &schema, &data);
            match st {
                nvre_nif::file::BlockStatus::Ok => {}
                nvre_nif::file::BlockStatus::Unknown => unknown += 1,
                nvre_nif::file::BlockStatus::Short { .. } => short += 1,
                nvre_nif::file::BlockStatus::Error(e) => {
                    err += 1;
                    // The first few errors, so a gap can be found.
                    if errors.len() < 12 {
                        let b = &f.blocks[i];
                        errors.push(format!(
                            "{}: {e} (block {} bytes at {})",
                            b.type_name,
                            b.range.end - b.range.start,
                            b.range.start
                        ));
                    }
                }
            }
        }
        if files as usize >= limit {
            break;
        }
    }
    println!(
        "files {files}, parse failures {parse_fail}, blocks {blocks}, unknown {unknown}, short {short}, error {err}"
    );
    for e in &errors {
        println!("  {e}");
    }
}
