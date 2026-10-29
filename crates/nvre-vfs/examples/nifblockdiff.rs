//! Find the blocks that do not re-encode, and say where they differ.
//!
//! A block that decodes but does not re-encode is a block the schema read
//! with the wrong layout. This finds them and prints the bytes that
//! differ, so the field at fault can be found.
//!
//! Usage:
//!     nifblockdiff <data dir> <schema dir>[:<layer> ...] <type> [limit]
//!
//! The data directory is scanned for the NIFs it holds; a single file's
//! own path is read instead when the first argument names a file.

use nvre_nif::Schema;
use rayon::prelude::*;
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

/// One block that does not re-encode.
struct Hit {
    path: String,
    index: usize,
    original: Vec<u8>,
    written: Vec<u8>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("a data folder");
    let schema_dir = args.next().expect("a schema directory");
    let want = args.next().expect("a block type");
    let limit: usize = args.next().unwrap_or_else(|| "5".into()).parse().unwrap();

    let mut dirs = schema_dir.split(':');
    let base = PathBuf::from(dirs.next().unwrap());
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&base).expect("the schema")
    } else {
        Schema::load_game(&base, &layers).expect("the schema")
    };

    // The files: the archives under the data directory, or one file.
    let files: Vec<(String, Vec<u8>)> = if PathBuf::from(&data_dir).is_file() {
        vec![(
            data_dir.clone(),
            std::fs::read(&data_dir).expect("the nif file"),
        )]
    } else {
        let vfs = open_vfs(&data_dir);
        vfs.paths(|p| p.to_lowercase().ends_with(".nif"))
            .par_iter()
            .filter_map(|p| {
                let data = vfs.read(p).and_then(|r| r.ok())?;
                Some((p.clone(), data))
            })
            .collect()
    };

    let hits: Vec<Hit> = files
        .par_iter()
        .flat_map_iter(|(path, data)| {
            let mut out: Vec<Hit> = Vec::new();
            let Ok(f) = nvre_nif::NifFile::parse(data, Some(&schema)) else {
                return out;
            };
            for (i, b) in f.blocks.iter().enumerate() {
                if b.type_name != want {
                    continue;
                }
                let Ok(st) = f.decode_block(i, &schema, data) else {
                    continue;
                };
                let Ok(ok) = f.reencode_decoded(&schema, data, i, &st) else {
                    continue;
                };
                if ok {
                    continue;
                }
                // The bytes as they were, and the bytes as they were written.
                let original = data[b.range.clone()].to_vec();
                let mut written = Vec::with_capacity(original.len());
                let lineage = {
                    let mut l = vec![b.type_name.clone()];
                    l.extend(
                        schema
                            .block_chain(&b.type_name)
                            .into_iter()
                            .map(str::to_string),
                    );
                    l
                };
                let fields = schema.block_fields_ref(&b.type_name).unwrap_or(&[]);
                if nvre_nif::encode::encode_fields(
                    &schema,
                    &f.header,
                    &lineage,
                    fields,
                    &st,
                    None,
                    &mut written,
                )
                .is_err()
                {
                    continue;
                }
                out.push(Hit {
                    path: path.clone(),
                    index: i,
                    original,
                    written,
                });
            }
            out
        })
        .collect();

    println!("{} blocks of {want} do not re-encode", hits.len());
    for hit in hits.iter().take(limit) {
        println!(
            "\n=== {} block {}: {} bytes in, {} bytes out",
            hit.path,
            hit.index,
            hit.original.len(),
            hit.written.len()
        );
        let n = hit.original.len().min(hit.written.len());
        let mut shown = false;
        for o in 0..n {
            if hit.original[o] != hit.written[o] {
                let lo = o.saturating_sub(8);
                let hi = (o + 8).min(n);
                println!("  first difference at offset {o}");
                println!("    original: {:02x?}", &hit.original[lo..hi]);
                println!("    written:  {:02x?}", &hit.written[lo..hi]);
                shown = true;
                break;
            }
        }
        if !shown && hit.original.len() != hit.written.len() {
            println!("  the same for the first {n} bytes, but the lengths differ");
        }
    }
}
