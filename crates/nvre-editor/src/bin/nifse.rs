//! How well does the schema explain the Special Edition's meshes?
use bsa105::Archive;
use nvre_nif::Schema;
use std::path::PathBuf;
fn main() {
    let data_dir = std::env::args().nth(1).unwrap();
    let schema_dir = std::env::args().nth(2).unwrap();
    let limit: usize = std::env::var("NIF_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = Schema::load_game(&PathBuf::from(base), &layers).unwrap();
    let (mut files, mut parse_fail, mut blocks) = (0u64, 0u64, 0u64);
    let (mut unknown, mut short, mut err) = (0u64, 0u64, 0u64);
    let mut mismatch = 0u64;
    let mut bs = std::collections::BTreeMap::new();
    let mut fails: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    let mut names: Vec<String> = std::fs::read_dir(&data_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("bsa"))
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    for name in &names {
        if !name.contains("Mesh") {
            continue;
        }
        let Ok(mut a) = Archive::open(PathBuf::from(&data_dir).join(name)) else {
            continue;
        };
        let paths: Vec<String> = a.entries().map(|map| map.path()).collect();
        for p in paths {
            if !p.to_lowercase().ends_with(".nif") {
                continue;
            }
            if files as usize >= limit {
                break;
            }
            let Ok(data) = a.extract_by_path(&p) else {
                continue;
            };
            let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
                parse_fail += 1;
                continue;
            };
            files += 1;
            *bs.entry(f.header.bs_version()).or_insert(0) += 1;
            for (i, _b) in f.blocks.iter().enumerate() {
                blocks += 1;
                let (status, _) = f.decode_block_status(i, &schema, &data);
                match status {
                    nvre_nif::BlockStatus::Unknown => {
                        unknown += 1;
                        *fails
                            .entry(format!("{} unknown", f.blocks[i].type_name))
                            .or_insert(0) += 1;
                    }
                    nvre_nif::BlockStatus::Short { .. } => {
                        short += 1;
                        *fails
                            .entry(format!("{} short", f.blocks[i].type_name))
                            .or_insert(0) += 1;
                    }
                    nvre_nif::BlockStatus::Error(e) => {
                        err += 1;
                        *fails
                            .entry(format!("{} error: {e}", f.blocks[i].type_name))
                            .or_insert(0) += 1;
                    }
                    _ => {}
                }
                if f.reencode_block(&schema, &data, i) == Ok(false) {
                    mismatch += 1;
                    *fails
                        .entry(format!("{} re-encodes differently", f.blocks[i].type_name))
                        .or_insert(0) += 1;
                }
            }
        }
        if files as usize >= limit {
            break;
        }
    }
    println!("files {files}, parse failures {parse_fail}");
    println!("bs_versions: {bs:?}");
    println!("blocks {blocks}, unknown {unknown}, short {short}, error {err}, re-encode mismatches {mismatch}");
    for (k, v) in &fails {
        println!("    {k}: {v}");
    }
}
