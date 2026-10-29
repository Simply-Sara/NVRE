//! Extract NIF files from a game's archives, by path substring.
//!
//! Usage: `nifextract <data folder> <path substring> [out dir]`
//!
//! Every matching file is written to the output directory under its own
//! name, so a schema can be measured against real bytes.

use nvre_vfs::Vfs;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("a data folder");
    let needle = args.next().expect("a path substring");
    let out_dir = args.next().unwrap_or_else(|| "/tmp/nifout".into());

    let dir = PathBuf::from(&data_dir);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("the data folder")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.ends_with(".bsa") || l.ends_with(".ba2")
        })
        .collect();
    names.sort();
    let vfs = Vfs::open(&dir, &names).expect("the archives");

    let needle = needle.to_lowercase();
    let mut n = 0;
    for p in vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        if !p.to_lowercase().contains(&needle) {
            continue;
        }
        let Some(r) = vfs.read(&p) else { continue };
        let Ok(data) = r else { continue };
        let name = p.replace(['\\', '/'], "_");
        let out = PathBuf::from(&out_dir).join(&name);
        if std::fs::write(&out, &data).is_ok() {
            println!("{p} -> {}", out.display());
            n += 1;
        }
        if n >= 40 {
            break;
        }
    }
    println!("{n} files written");
}
