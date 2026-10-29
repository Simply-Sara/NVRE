//! Check NVRE's schema against the file's own block table.
//!
//! A NIF from 20.2.0.5 on records each block's size. The schema has to
//! explain exactly that many bytes: fewer is a "short" block, more is an
//! error. This reports the blocks where the two disagree, by type and by
//! how much they differ, so the wrong field can be found.
//!
//! Usage:
//!     nifsize <data folder> <schema dir>[:<layer> ...]

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

/// How a type's blocks went wrong: how many, and by how much.
#[derive(Debug, Default, Clone)]
struct Summary {
    /// How many blocks of this type were checked.
    blocks: usize,
    /// How many of them did not use exactly the bytes the file records.
    bad: usize,
    /// How many bytes short they were, by how many blocks.
    short: BTreeMap<i64, usize>,
    /// The errors, by message.
    errors: BTreeMap<String, usize>,
    /// An example of each kind of failure.
    examples: BTreeMap<String, String>,
}

impl Summary {
    fn note(&mut self, key: String, path: &str) {
        self.examples.entry(key).or_insert_with(|| path.to_string());
    }
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
    println!("checking {} nif files...", paths.len());

    // The failures, by type.
    let sums: Mutex<BTreeMap<String, Summary>> = Mutex::new(BTreeMap::new());

    paths.par_iter().for_each(|p| {
        let Some(r) = vfs.read(p) else { return };
        let Ok(data) = r else { return };
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
            return;
        };
        let mut local: BTreeMap<String, Summary> = BTreeMap::new();
        for (i, b) in f.blocks.iter().enumerate() {
            let (status, _) = f.decode_block_status(i, &schema, &data);
            let s = local.entry(b.type_name.clone()).or_default();
            s.blocks += 1;
            match status {
                BlockStatus::Ok => {}
                BlockStatus::Short { consumed, size } => {
                    s.bad += 1;
                    let delta = size as i64 - consumed as i64;
                    *s.short.entry(delta).or_insert(0) += 1;
                    s.note(format!("short {delta}"), p);
                }
                BlockStatus::Error(e) => {
                    s.bad += 1;
                    let msg = e.to_string();
                    *s.errors.entry(msg.clone()).or_insert(0) += 1;
                    s.note(format!("error {msg}"), p);
                }
                BlockStatus::Unknown => {
                    s.bad += 1;
                    *s.errors.entry("unknown".to_string()).or_insert(0) += 1;
                    s.note("unknown".to_string(), p);
                }
            }
        }
        let mut all = sums.lock().unwrap();
        for (ty, s) in local {
            let e = all.entry(ty).or_default();
            e.blocks += s.blocks;
            e.bad += s.bad;
            for (d, n) in s.short {
                *e.short.entry(d).or_insert(0) += n;
            }
            for (m, n) in s.errors {
                *e.errors.entry(m).or_insert(0) += n;
            }
            for (k, v) in s.examples {
                e.examples.entry(k).or_insert(v);
            }
        }
    });

    let sums = sums.lock().unwrap();
    let total: usize = sums.values().map(|s| s.blocks).sum();
    let bad: usize = sums.values().map(|s| s.bad).sum();
    println!("\n{total} blocks checked, {bad} disagreements");
    for (ty, s) in sums.iter() {
        if s.bad == 0 {
            continue;
        }
        println!("\n  {ty}: {} of {} blocks wrong", s.bad, s.blocks);
        for (d, n) in &s.short {
            let ex = s
                .examples
                .get(&format!("short {d}"))
                .cloned()
                .unwrap_or_default();
            println!("    {n:6} x short {d} bytes   (e.g. {ex})");
        }
        for (m, n) in &s.errors {
            let ex = s
                .examples
                .get(&format!("error {m}"))
                .cloned()
                .unwrap_or_default();
            println!("    {n:6} x {m}   (e.g. {ex})");
        }
    }
}
