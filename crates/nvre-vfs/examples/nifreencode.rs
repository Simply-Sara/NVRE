//! S1-1, exhaustively: does every block of every NIF in the game re-encode to
//! the bytes it was read from?
//!
//! The one measurement in the parity plan that needs no external reference:
//! the encoder is the decoder's inverse, so a block that decodes but does not
//! re-encode is a block the schema read with the wrong layout — which is what
//! a `bs_version` condition written for another game does, and it silently
//! shifts every field after it.
//!
//! It lives here rather than in `nvre-editor` because it is the measurement
//! that has to run over a whole game's meshes — Fallout: New Vegas alone holds
//! 20,542 of them, Fallout 4 224,734. `nvre-editor` depends on Bevy, so a
//! binary in it takes minutes to build and pulls a GUI framework into a batch
//! job. This crate depends on nothing but the archive reader, so the checker
//! builds in seconds and runs the same corpus.
//!
//! The files are independent, so they are checked in parallel, and each block
//! is decoded once: the status and the re-encode read the same struct, and
//! decoding twice doubles the cost of the more expensive half. For a routine
//! check use `nifsample`, which stratifies a sample instead and answers in
//! seconds.
//!
//! Usage: `nifreencode <data dir> <schema dir[:layers]> [path substring]`
//!
//! The schema chain differs per game — FNV and FO3 pass the base schema
//! directory alone, the others pass `base:<version dir>` — and the same binary
//! takes either, because `Schema::load_game` does.

use nvre_nif::Schema;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

/// Open every archive in `data_dir`, so each game's meshes are reachable.
///
/// The VFS's default list is Fallout's, which is why a Skyrim or Fallout 4
/// folder would otherwise report no NIFs at all: their archives are named
/// differently and none of them is opened. So every archive in the folder is
/// opened, in name order.
///
/// That is not the game's own load order — a mod's archive should be read over
/// the base game's, and name order does not guarantee that — so this is right
/// for asking *what the archives hold* and wrong for asking *which of two files
/// a reference resolves to*. S0-1 answers the second question; this checker
/// answers the first.
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
    if names.is_empty() {
        // A folder with no archives still has loose files; the VFS reads those
        // from the folder itself.
        eprintln!("no .bsa or .ba2 in {data_dir}; reading loose files only");
    }
    nvre_vfs::Vfs::open(&dir, &names).expect("the archives")
}

/// One file's counts and reasons, so the parallel pass can return them and the
/// totals are a sum rather than a shared counter.
#[derive(Default)]
struct Found {
    files: u64,
    parse_fail: u64,
    blocks: u64,
    unknown: u64,
    short: u64,
    err: u64,
    mismatch: u64,
    /// What went wrong, by block type: a type that fails everywhere is one
    /// layout bug, and fifty separate ones are fifty.
    reasons: BTreeMap<String, u64>,
    /// One example file per reason, so a failure can be opened and looked at.
    examples: BTreeMap<String, String>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("a data folder");
    let schema_dir = args.next().expect("a schema directory");
    // A path filter, so one data subtree can be surveyed inside a timeout.
    let filter = args.next().unwrap_or_default().to_lowercase();

    // The schema chain: the first path is the base, the rest are layers.
    let mut dirs = schema_dir.split(':');
    let base = PathBuf::from(dirs.next().unwrap());
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&base).expect("the schema")
    } else {
        Schema::load_game(&base, &layers).expect("the schema")
    };

    let vfs = open_vfs(&data_dir);
    let paths = vfs.paths(|p| {
        p.to_lowercase().ends_with(".nif")
            && (filter.is_empty() || p.to_lowercase().contains(&filter))
    });
    println!("the archive holds {} nif files", paths.len());

    let started = Instant::now();
    let found: Found = paths
        .par_iter()
        .fold(Found::default, |mut acc, p| {
            let Some(r) = vfs.read(p) else { return acc };
            let Ok(data) = r else { return acc };
            let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
                acc.parse_fail += 1;
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
            a.parse_fail += b.parse_fail;
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
    println!("files {}, parse failures {}", found.files, found.parse_fail);
    println!(
        "blocks {}, unknown {}, short {}, error {}, re-encode mismatches {}",
        found.blocks, found.unknown, found.short, found.err, found.mismatch
    );
    println!(
        "in {secs:.1}s on {} threads ({} files/s)",
        rayon::current_num_threads(),
        (found.files as f32 / secs) as u64
    );
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
