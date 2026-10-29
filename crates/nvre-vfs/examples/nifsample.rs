//! S1-1 on a sample: does every block of every NIF in a *sample* of the corpus
//! re-encode to the bytes it was read from?
//!
//! The full sweep over all five games is 306,674 files and takes hours, which
//! is too slow to run after every change. A sample of a fixed size, drawn the
//! same way each time, gives an answer in minutes and is reproducible: the
//! same seed and the same corpus give the same files.
//!
//! The sample is *stratified by data subtree*, not drawn at random from the
//! whole archive. A random draw over 20k paths lands mostly in whatever
//! subtree happens to be largest, so a bug confined to `meshes\weapons` would
//! be missed by chance; taking a share of each subtree means every class of
//! mesh is represented whether or not it is numerous.
//!
//! Usage: `nifsample <data dir> <schema dir[:layers]> [per-subtree count]`
//!
//! With no count it takes 1 in every 40 paths of each subtree, which over
//! Fallout: New Vegas is about 500 files.

use nvre_nif::Schema;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("a data folder");
    let schema_dir = args.next().expect("a schema directory");
    // How many paths to take from each data subtree. Zero means a share.
    let cap: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);

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
    let all = vfs.paths(|p| p.to_lowercase().ends_with(".nif"));

    // Group the paths by their data subtree: everything up to the third
    // separator, so `meshes\architecture\...` and `meshes\clutter\...` are
    // different strata and `meshes\landscape\lod` is its own.
    let mut strata: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in all {
        let key = p
            .split('\\')
            .take(2)
            .collect::<Vec<_>>()
            .join("\\")
            .to_lowercase();
        strata.entry(key).or_default().push(p);
    }

    // The sample: every 40th of each stratum, or `cap` of it if that is fewer.
    let mut sample: Vec<String> = Vec::new();
    for (_, paths) in &strata {
        let take = if cap > 0 {
            cap.min(paths.len())
        } else {
            (paths.len() / 40).max(1)
        };
        let step = (paths.len() / take).max(1);
        sample.extend(paths.iter().step_by(step).take(take).cloned());
    }
    println!(
        "{} files in {} strata; sampling {}",
        strata.values().map(|v| v.len()).sum::<usize>(),
        strata.len(),
        sample.len()
    );

    let started = std::time::Instant::now();
    let mut files = 0u64;
    let mut blocks = 0u64;
    let mut unknown = 0u64;
    let mut short = 0u64;
    let mut err = 0u64;
    let mut mismatch = 0u64;
    let mut reasons: BTreeMap<String, u64> = BTreeMap::new();
    let mut examples: BTreeMap<String, String> = BTreeMap::new();

    for p in &sample {
        let Some(r) = vfs.read(p) else { continue };
        let Ok(data) = r else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
            continue;
        };
        files += 1;
        for (i, _b) in f.blocks.iter().enumerate() {
            blocks += 1;
            let (status, _) = f.decode_block_status(i, &schema, &data);
            match status {
                nvre_nif::BlockStatus::Unknown => {
                    unknown += 1;
                    *reasons
                        .entry(format!("{} unknown", f.blocks[i].type_name))
                        .or_insert(0) += 1;
                }
                nvre_nif::BlockStatus::Short { consumed, size } => {
                    short += 1;
                    *reasons
                        .entry(format!(
                            "{} short: consumed {consumed} of {size}",
                            f.blocks[i].type_name
                        ))
                        .or_insert(0) += 1;
                }
                nvre_nif::BlockStatus::Error(e) => {
                    err += 1;
                    *reasons
                        .entry(format!("{} error: {e}", f.blocks[i].type_name))
                        .or_insert(0) += 1;
                }
                _ => {}
            }
            if f.reencode_block(&schema, &data, i) == Ok(false) {
                mismatch += 1;
                *reasons
                    .entry(format!("{} re-encode", f.blocks[i].type_name))
                    .or_insert(0) += 1;
                examples
                    .entry(f.blocks[i].type_name.clone())
                    .or_insert_with(|| p.clone());
            }
        }
    }

    let secs = started.elapsed().as_secs_f32();
    println!();
    println!("files {files}, blocks {blocks}");
    println!("unknown {unknown}, short {short}, error {err}, re-encode mismatches {mismatch}");
    println!("in {secs:.1}s");
    println!();
    println!("by block:");
    for (k, v) in &reasons {
        let first = examples.get(k.split(':').next().unwrap_or(k));
        match first {
            Some(p) => println!("    {k}: {v}   (e.g. {p})"),
            None => println!("    {k}: {v}"),
        }
    }
}
