//! S1-1: does every block of every NIF re-encode to the bytes it was read
//! from?
//!
//! The one measurement in the parity plan that needs no external reference:
//! the encoder is the decoder's inverse, so a block that decodes but does not
//! re-encode is a block the schema read with the wrong layout — which is what
//! a `bs_version` condition written for another game does, and it silently
//! shifts every field after it.
//!
//! It lives here rather than in `nvre-editor` because it is the measurement
//! that has to run over a whole game's meshes — Fallout: New Vegas alone holds
//! 20,542 of them. `nvre-editor` depends on Bevy, so a binary in it takes
//! minutes to build and pulls a GUI framework into a batch job. This crate
//! depends on nothing but the archive reader, so the checker builds in seconds
//! and runs the same corpus.
//!
//! Usage: `nifreencode <data dir> <schema dir[:layers]> [path substring]`
//!
//! The schema chain differs per game — FNV and FO3 pass the base schema
//! directory alone, the others pass `base:<version dir>` — and the same binary
//! takes either, because `Schema::load_game` does. The archive list is the
//! VFS's default, which is Fallout's; a game whose archives are named
//! differently will show fewer files until the profile's own list is passed in.

use nvre_nif::Schema;
use std::path::PathBuf;
use std::time::Instant;

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

    let vfs =
        nvre_vfs::Vfs::open_default(PathBuf::from(&data_dir).as_path()).expect("the archives");
    let paths = vfs.paths(|p| {
        p.to_lowercase().ends_with(".nif")
            && (filter.is_empty() || p.to_lowercase().contains(&filter))
    });
    println!("the archive holds {} nif files", paths.len());

    let started = Instant::now();
    // Where the time goes, separated so a slow run says which half it is: the
    // archive read (decompression) or the schema work (decode + re-encode).
    let mut read_time = std::time::Duration::ZERO;
    let mut work_time = std::time::Duration::ZERO;
    let mut files = 0u64;
    let mut parse_fail = 0u64;
    let mut blocks = 0u64;
    let mut unknown = 0u64;
    let mut short = 0u64;
    let mut err = 0u64;
    let mut mismatch = 0u64;
    // What went wrong, by block type: a type that fails everywhere is one
    // layout bug, and fifty separate ones are fifty.
    let mut reasons: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    // One example file per reason, so a failure can be opened and looked at.
    let mut examples: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    for p in &paths {
        let t = Instant::now();
        let Some(r) = vfs.read(p) else { continue };
        let Ok(data) = r else { continue };
        read_time += t.elapsed();
        let t = Instant::now();
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
            parse_fail += 1;
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
        work_time += t.elapsed();
        if files % 2000 == 0 {
            println!("  {files} files, {mismatch} mismatches so far");
        }
    }

    let secs = started.elapsed().as_secs_f32();
    println!();
    println!("files {files}, parse failures {parse_fail}");
    println!(
        "blocks {blocks}, unknown {unknown}, short {short}, error {err}, re-encode mismatches {mismatch}"
    );
    println!("in {secs:.1}s ({} files/s)", (files as f32 / secs) as u64);
    println!(
        "archive read {:.1}s, schema work {:.1}s",
        read_time.as_secs_f32(),
        work_time.as_secs_f32()
    );
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
