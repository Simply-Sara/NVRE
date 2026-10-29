//! S1-4: Find and inspect all Short or Error blocks across the corpus.
//!
//! A Short block means the schema consumed fewer bytes than the block holds —
//! the signature of a layout written for another game version or variant.

use nvre_nif::{BlockStatus, Schema};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

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
        "scanning {} nif files for Short / Error blocks...",
        paths.len()
    );

    let issues: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());
    let examples: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

    paths.par_iter().for_each(|p| {
        let Some(r) = vfs.read(p) else { return };
        let Ok(data) = r else { return };
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
            return;
        };
        for (i, b) in f.blocks.iter().enumerate() {
            let (status, _) = f.decode_block_status(i, &schema, &data);
            let key = match status {
                BlockStatus::Short { consumed, size } => {
                    format!("{} short: consumed {consumed} of {size}", b.type_name)
                }
                BlockStatus::Error(e) => {
                    format!("{} error: {e}", b.type_name)
                }
                BlockStatus::Unknown => {
                    format!("{} unknown", b.type_name)
                }
                _ => continue,
            };
            *issues.lock().unwrap().entry(key.clone()).or_insert(0) += 1;
            examples
                .lock()
                .unwrap()
                .entry(key)
                .or_insert_with(|| p.clone());
        }
    });

    println!("\nS1-4 Short / Error Report:");
    for (k, count) in &*issues.lock().unwrap() {
        let ex = examples.lock().unwrap().get(k).cloned().unwrap_or_default();
        println!("  {count:5} x {k}   (e.g. {ex})");
    }
}
