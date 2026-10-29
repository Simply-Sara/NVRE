//! S1-2: Version and bs_version distribution histograms per game.
//!
//! Prints the exact counts of (version, user_version, bs_version) across
//! every NIF file in the game's data archives/folder.

use nvre_nif::Schema;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn open_vfs(data_dir: &str) -> nvre_vfs::Vfs {
    let dir = PathBuf::from(data_dir);
    let mut names: Vec<String> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                let l = n.to_ascii_lowercase();
                l.ends_with(".bsa") || l.ends_with(".ba2")
            })
            .collect(),
        Err(e) => panic!("{data_dir}: {e}"),
    };
    names.sort();
    nvre_vfs::Vfs::open(&dir, &names).expect("the archives")
}

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("a data folder");
    let schema_dir = args.next().expect("a schema directory");

    let mut dirs = schema_dir.split(':');
    let base = PathBuf::from(dirs.next().unwrap());
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&base).expect("the schema")
    } else {
        Schema::load_game(&base, &layers).expect("the schema")
    };

    let vfs = open_vfs(&data_dir);
    let paths = vfs.paths(|p| p.to_lowercase().ends_with(".nif"));
    println!(
        "scanning {} nif files for version histogram...",
        paths.len()
    );

    let mut histograms: BTreeMap<(u32, u32, u32), u64> = BTreeMap::new();
    let mut parse_fails = 0u64;

    for p in &paths {
        let Some(r) = vfs.read(p) else { continue };
        let Ok(data) = r else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
            parse_fails += 1;
            continue;
        };
        let h = &f.header;
        let key = (h.version, h.user_version, h.bs_version());
        *histograms.entry(key).or_insert(0) += 1;
    }

    println!("\nVersion Histogram (version, user_version, bs_version) -> count:");
    for ((v, uv, bv), count) in &histograms {
        println!(
            "  version 0x{:08x} ({v}), user_version {uv}, bs_version {bv} : {count} files",
            v
        );
    }
    if parse_fails > 0 {
        println!("  parse failures: {parse_fails}");
    }
}
