//! Which file makes the corpus check take forever?
//!
//! Skyrim SE's sample ran 95 minutes of CPU where Skyrim LE's took 1.4 seconds,
//! for the same number of files. That is not a mesh being large; it is a block
//! whose read loops, or whose length expression asks for an enormous
//! allocation. This prints each path *before* checking it and stops on the
//! first one that overruns, so the file names itself.
//!
//! Usage: `nifslow <data dir> <schema dir[:layers]> [seconds per file]`

use nvre_nif::Schema;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("a data folder");
    let schema_dir = args.next().expect("a schema directory");
    // How long one file may take before it is called the culprit.
    let budget: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);

    let mut dirs = schema_dir.split(':');
    let base = PathBuf::from(dirs.next().unwrap());
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&base).expect("the schema")
    } else {
        Schema::load_game(&base, &layers).expect("the schema")
    };

    let dir = PathBuf::from(&data_dir);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.ends_with(".bsa") || l.ends_with(".ba2")
        })
        .collect();
    names.sort();
    let vfs = nvre_vfs::Vfs::open(&dir, &names).expect("the archives");
    let paths = vfs.paths(|p| p.to_lowercase().ends_with(".nif"));
    println!("{} files; one may take up to {budget}s each", paths.len());

    // One file at a time, single-threaded and in order, so the culprit is the
    // last line printed.
    let mut slowest: (String, f32) = (String::new(), 0.0);
    for (n, p) in paths.iter().enumerate() {
        println!("[{n}] {p}");
        use std::io::Write;
        std::io::stdout().flush().ok();
        let Some(r) = vfs.read(p) else { continue };
        let Ok(data) = r else { continue };
        let t = Instant::now();
        let Ok(f) = nvre_nif::NifFile::parse(&data, Some(&schema)) else {
            continue;
        };
        for i in 0..f.blocks.len() {
            let (_, decoded) = f.decode_block_status(i, &schema, &data);
            if let Some(st) = &decoded {
                let _ = f.reencode_decoded(&schema, &data, i, st);
            }
        }
        let secs = t.elapsed().as_secs_f32();
        if secs > slowest.1 {
            slowest = (p.clone(), secs);
        }
        if t.elapsed() > Duration::from_secs(budget) {
            println!("\nSLOWEST SO FAR: {} took {:.1}s", slowest.0, slowest.1);
            println!(">>> {p} took {secs:.1}s — this is the one");
            return;
        }
    }
    println!(
        "\nno file overran {budget}s; slowest was {} at {:.1}s",
        slowest.0, slowest.1
    );
}
