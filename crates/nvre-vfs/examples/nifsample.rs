//! S1-1 on a sample: does every block of every NIF in a *sample* of the corpus
//! re-encode to the bytes it was read from?
//!
//! The exhaustive sweep over all five games is 306,674 files and takes hours,
//! which is too slow to run after every change. A sample of a fixed size, drawn
//! the same way each time, gives an answer in minutes and is reproducible: the
//! same corpus gives the same files.
//!
//! The sample is *stratified by data subtree*, not drawn at random from the
//! whole archive. A random draw over 20k paths lands mostly in whatever subtree
//! happens to be largest, so a bug confined to `meshes\weapons` would be missed
//! by chance; taking a share of each subtree means every class of mesh is
//! represented whether or not it is numerous.
//!
//! The files are independent, so they are checked in parallel — one core at a
//! time is the difference between a minute and half an hour over a quarter of a
//! million files.
//!
//! Usage: `nifsample <data dir> <schema dir[:layers]> [per-subtree count]`
//!
//! With no count it takes 1 in every 40 paths of each subtree.

use nvre_nif::Schema;
use rayon::prelude::*;
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

    // Group the paths by their data subtree: everything up to the second
    // separator, so `meshes\architecture` and `meshes\clutter` are different
    // strata and a bug confined to one of them is still represented.
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

    // One file's result: the counts and the reasons, so the parallel pass can
    // return them and the totals are a sum rather than a shared counter.
    #[derive(Default)]
    struct Found {
        files: u64,
        blocks: u64,
        unknown: u64,
        short: u64,
        err: u64,
        mismatch: u64,
        reasons: BTreeMap<String, u64>,
        examples: BTreeMap<String, String>,
    }

    let started = std::time::Instant::now();
    let found: Found = sample
        .par_iter()
        .fold(Found::default, |mut acc, p| {
            let Some(r) = vfs.read(p) else { return acc };
            let Ok(data) = r else { return acc };
            // Each block is decoded once: the status and the re-encode read the
            // same struct, and decoding twice doubles the cost of the more
            // expensive half.
            let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
                return acc;
            };
            acc.files += 1;
            for (i, _b) in f.blocks.iter().enumerate() {
                acc.blocks += 1;
                let (status, decoded) = f.decode_block_status(i, &schema, &data);
                match status {
                    nvre_nif::BlockStatus::Unknown => {
                        acc.unknown += 1;
                        *acc.reasons
                            .entry(format!("{} unknown", f.blocks[i].type_name))
                            .or_insert(0) += 1;
                    }
                    nvre_nif::BlockStatus::Short { consumed, size } => {
                        acc.short += 1;
                        *acc.reasons
                            .entry(format!(
                                "{} short: consumed {consumed} of {size}",
                                f.blocks[i].type_name
                            ))
                            .or_insert(0) += 1;
                    }
                    nvre_nif::BlockStatus::Error(e) => {
                        acc.err += 1;
                        *acc.reasons
                            .entry(format!("{} error: {e}", f.blocks[i].type_name))
                            .or_insert(0) += 1;
                    }
                    _ => {}
                }
                // The re-encode, on the struct the status already decoded.
                let ok = match &decoded {
                    Some(st) => f.reencode_decoded(&schema, &data, i, st),
                    None => f.reencode_block(&schema, &data, i),
                };
                if ok == Ok(false) {
                    acc.mismatch += 1;
                    *acc.reasons
                        .entry(format!("{} re-encode", f.blocks[i].type_name))
                        .or_insert(0) += 1;
                    acc.examples
                        .entry(f.blocks[i].type_name.clone())
                        .or_insert_with(|| p.clone());
                }
            }
            acc
        })
        .reduce(Found::default, |mut a, b| {
            a.files += b.files;
            a.blocks += b.blocks;
            a.unknown += b.unknown;
            a.short += b.short;
            a.err += b.err;
            a.mismatch += b.mismatch;
            for (k, v) in b.reasons {
                *a.reasons.entry(k).or_insert(0) += v;
            }
            for (k, v) in b.examples {
                a.examples.entry(k).or_insert(v);
            }
            a
        });

    let secs = started.elapsed().as_secs_f32();
    println!();
    println!("files {}, blocks {}", found.files, found.blocks);
    println!(
        "unknown {}, short {}, error {}, re-encode mismatches {}",
        found.unknown, found.short, found.err, found.mismatch
    );
    println!("in {secs:.1}s on {} threads", rayon::current_num_threads());
    println!();
    println!("by block:");
    for (k, v) in &found.reasons {
        let first = found.examples.get(k.split(':').next().unwrap_or(k));
        match first {
            Some(p) => println!("    {k}: {v}   (e.g. {p})"),
            None => println!("    {k}: {v}"),
        }
    }
}
