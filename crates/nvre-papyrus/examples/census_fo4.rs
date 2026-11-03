//! Parse every `.pex` in the user's Fallout 4 archives and report what the
//! reader makes of them.
//!
//! Fallout 4 is the little-endian dialect, and its object record carries
//! fields Skyrim's does not — a const flag, a struct table, and a const flag
//! per variable. This is the census that proves those offsets, the way
//! `census` proves Skyrim's.
//!
//! ```text
//! cargo run --release -p nvre-papyrus --example census_fo4 -- "<Data dir>/Fallout4 - Misc.ba2"
//! ```
use ba2::Archive;
use nvre_papyrus::PexFile;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: census_fo4 <BA2 path>");
    let mut archive = Archive::open(&path).expect("the archive");
    println!("{path}: {} entries", archive.files.len());

    let scripts: Vec<String> = archive
        .paths()
        .filter(|p| {
            let lower = p.to_ascii_lowercase();
            lower.starts_with("scripts\\") && lower.ends_with(".pex")
        })
        .map(|s| s.to_string())
        .collect();
    println!("{} scripts", scripts.len());

    let mut ok = 0usize;
    let mut round_trip = 0usize;
    let mut versions: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut size_mismatch = 0usize;
    let mut structs = 0usize;
    let mut objects = 0usize;
    let mut functions = 0usize;
    let mut instructions = 0usize;

    for p in &scripts {
        let entry = match archive.find(p) {
            Some(e) => e.clone(),
            None => continue,
        };
        let bytes = match archive.extract(&entry) {
            Ok(b) => b,
            Err(e) => {
                failures.push((p.clone(), format!("extract: {e}")));
                continue;
            }
        };
        let file = match PexFile::parse(&bytes) {
            Ok(f) => f,
            Err(e) => {
                failures.push((p.clone(), format!("parse: {e}")));
                continue;
            }
        };
        ok += 1;
        *versions
            .entry(format!(
                "{}.{} game={} {:?}",
                file.header.major, file.header.minor, file.header.game_id, file.endianness
            ))
            .or_default() += 1;
        if !file.size_mismatches().is_empty() {
            size_mismatch += 1;
        }
        for o in &file.objects {
            objects += 1;
            structs += o.structs.len();
            for s in &o.states {
                for f in &s.functions {
                    functions += 1;
                    instructions += f.instructions.len();
                }
            }
            for prop in &o.properties {
                if let Some(g) = &prop.getter {
                    functions += 1;
                    instructions += g.instructions.len();
                }
                if let Some(s) = &prop.setter {
                    functions += 1;
                    instructions += s.instructions.len();
                }
            }
        }
        match file.write() {
            Ok(w) if w == bytes => round_trip += 1,
            Ok(w) => {
                let at = w.iter().zip(&bytes).position(|(a, b)| a != b);
                failures.push((
                    p.clone(),
                    format!("round-trip at {at:?} ({} in, {} out)", bytes.len(), w.len()),
                ));
            }
            Err(e) => failures.push((p.clone(), format!("write: {e}"))),
        }
    }

    println!("\nparsed      {ok} / {}", scripts.len());
    println!("round-trip  {round_trip} / {ok}");
    println!(
        "objects {objects}, structs {structs}, functions {functions}, instructions {instructions}"
    );
    println!("size-field mismatches: {size_mismatch}");
    println!("\nversions:");
    for (k, v) in &versions {
        println!("  {k}: {v}");
    }
    println!("\nfailures: {}", failures.len());
    for (f, e) in failures.iter().take(20) {
        println!("  {f}: {e}");
    }
}
