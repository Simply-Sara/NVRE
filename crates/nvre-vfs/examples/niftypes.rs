//! Which block types does a game's meshes use that the schema does not define?
//!
//! S1-3, measured without decoding anything: walk every NIF's block type names
//! and report the ones the schema has no definition for. A type that is
//! undefined is a block the reader cannot read at all, which is a different
//! and cheaper thing to fix than a block it reads wrongly — so it is worth
//! knowing the list before spending time on the re-encode check.
//!
//! Usage: `niftypes <data dir> <schema dir[:layers]> [path substring]`

use nvre_nif::Schema;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("a data folder");
    let schema_dir = args.next().expect("a schema directory");
    let filter = args.next().unwrap_or_default().to_lowercase();

    let mut dirs = schema_dir.split(':');
    let base = PathBuf::from(dirs.next().unwrap());
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&base).expect("the schema")
    } else {
        Schema::load_game(&base, &layers).expect("the schema")
    };

    let dir = PathBuf::from(&data_dir);
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
    let vfs = nvre_vfs::Vfs::open(&dir, &names).expect("the archives");
    let paths = vfs.paths(|p| {
        p.to_lowercase().ends_with(".nif")
            && (filter.is_empty() || p.to_lowercase().contains(&filter))
    });

    // Every block type the corpus names, and how many files use it.
    let mut used: BTreeMap<String, u64> = BTreeMap::new();
    let mut files = 0u64;
    for p in &paths {
        let Some(r) = vfs.read(p) else { continue };
        let Ok(data) = r else { continue };
        // Only the header's type-name list is read, so no block is decoded:
        // this is a question about the schema's coverage, not about layouts.
        let Ok(f) = nvre_nif::NifFile::parse(&data, None) else {
            continue;
        };
        files += 1;
        for b in &f.blocks {
            *used.entry(b.type_name.clone()).or_insert(0) += 1;
        }
    }

    let mut missing: Vec<(&String, u64)> = used
        .iter()
        .filter(|(t, _)| !schema.blocks.contains_key(t.as_str()))
        .map(|(t, n)| (t, *n))
        .collect();
    missing.sort_by_key(|(_, n)| std::cmp::Reverse(*n));

    println!("{files} files, {} distinct block types", used.len());
    println!("{} types the schema does not define:", missing.len());
    for (t, n) in &missing {
        println!("    {n:8}  {t}");
    }
}
