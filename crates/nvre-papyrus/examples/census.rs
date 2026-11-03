//! Parse every `.pex` in the user's Skyrim archives and report what the reader
//! makes of them: how many, of what version, how many round-trip byte for byte,
//! and every file the reader refuses.
//!
//! This is the crate's acceptance test. The same shape of evidence the
//! obscript compiler uses: not a spec, and not a test written to match the
//! code, but the game's own shipped bytes.
//!
//! ```text
//! PEX_ARCHIVE="<Data dir>/Skyrim - Misc.bsa" cargo run --release -p nvre-papyrus --example census
//! ```
use bsa105::Archive;
use nvre_papyrus::PexFile;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("PEX_ARCHIVE").ok())
        .expect("usage: census <BSA path>");

    let mut archive = Archive::open(&path).expect("the archive");
    println!("{}: {} folders", path, archive.folders.len());

    // Collect the script paths first: `extract_by_path` needs `&mut self`, and
    // `entries` borrows the archive.
    let scripts: Vec<String> = archive
        .entries()
        .map(|e| e.path())
        .filter(|p| {
            let lower = p.to_ascii_lowercase();
            lower.starts_with("scripts\\") && lower.ends_with(".pex")
        })
        .collect();
    println!("{} scripts", scripts.len());

    let mut ok = 0usize;
    let mut round_trip = 0usize;
    let mut versions: BTreeMap<String, usize> = BTreeMap::new();
    let mut opcodes: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut values: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut objects = 0usize;
    let mut functions = 0usize;
    let mut instructions = 0usize;
    let mut size_mismatch = 0usize;
    let mut failures: Vec<(String, String)> = Vec::new();

    for p in &scripts {
        let bytes = match archive.extract_by_path(p) {
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

        // Every object's `size` field must agree with the bytes it holds.
        if !file.size_mismatches().is_empty() {
            size_mismatch += 1;
            let m = file.size_mismatches();
            failures.push((
                p.clone(),
                format!("size field says {} but the object holds {}", m[0].1, m[0].2),
            ));
        }

        for o in &file.objects {
            objects += 1;
            for f in object_functions(o) {
                functions += 1;
                for i in &f.instructions {
                    instructions += 1;
                    *opcodes
                        .entry(nvre_papyrus::opcodes::name(i.opcode))
                        .or_default() += 1;
                    for v in i.args.iter().chain(&i.varargs) {
                        let k = match v {
                            nvre_papyrus::Value::None => "none",
                            nvre_papyrus::Value::Identifier(_) => "identifier",
                            nvre_papyrus::Value::String(_) => "string",
                            nvre_papyrus::Value::Int(_) => "int",
                            nvre_papyrus::Value::Float(_) => "float",
                            nvre_papyrus::Value::Bool(_) => "bool",
                        };
                        *values.entry(k).or_default() += 1;
                    }
                }
            }
        }

        // The real check: does the model write back the bytes it was read from?
        match file.write() {
            Ok(w) if w == bytes => round_trip += 1,
            Ok(w) => {
                let at = w.iter().zip(&bytes).position(|(a, b)| a != b);
                failures.push((
                    p.clone(),
                    format!(
                        "round-trip differs at {at:?} ({} bytes in, {} out)",
                        bytes.len(),
                        w.len()
                    ),
                ));
            }
            Err(e) => failures.push((p.clone(), format!("write: {e}"))),
        }
    }

    println!("\nparsed      {ok} / {}", scripts.len());
    println!("round-trip  {round_trip} / {ok}");
    println!("objects {objects}, functions {functions}, instructions {instructions}");
    println!("size-field mismatches: {size_mismatch}");
    println!("\nversions:");
    for (k, v) in &versions {
        println!("  {k}: {v}");
    }
    println!("\nopcodes:");
    for (k, v) in &opcodes {
        println!("  {k}: {v}");
    }
    println!("\nvalues:");
    for (k, v) in &values {
        println!("  {k}: {v}");
    }
    println!("\nfailures: {}", failures.len());
    for (f, e) in failures.iter().take(30) {
        println!("  {f}: {e}");
    }
}

fn object_functions(o: &nvre_papyrus::Object) -> Vec<&nvre_papyrus::pex::Function> {
    let mut v = Vec::new();
    for p in &o.properties {
        if let Some(g) = &p.getter {
            v.push(g);
        }
        if let Some(s) = &p.setter {
            v.push(s);
        }
    }
    for s in &o.states {
        for f in &s.functions {
            v.push(f);
        }
    }
    v
}
