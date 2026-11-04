//! Probe: which classes does `callstatic` name, and how often?
//!
//! ```text
//! cargo run --release --example callstaticcensus -- "Skyrim - Misc.bsa"
//! ```
//!
//! The emitter treats `game`, `debug`, `utility` and `math` as the classes a
//! **bare** call can resolve to. If the corpus writes `callstatic` against other
//! classes, a bare call to one of *those* names is a call this does not model,
//! and it lands in the `callmethod vs callstatic` bucket instead.
use nvre_papyrus::{opcodes, PexFile, Value};
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 2 {
        eprintln!("usage: callstaticcensus <archive.bsa>");
        return;
    }
    let mut archive = bsa105::Archive::open(&a[1]).unwrap();
    let paths: Vec<String> = archive
        .entries()
        .map(|e| e.path())
        .filter(|p| p.to_ascii_lowercase().ends_with(".pex"))
        .collect();

    // class -> function -> count
    let mut by_class: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for path in &paths {
        let Ok(bytes) = archive.extract_by_path(path) else {
            continue;
        };
        let Ok(f) = PexFile::parse(&bytes) else {
            continue;
        };
        for obj in &f.objects {
            for st in &obj.states {
                for func in &st.functions {
                    for ins in &func.instructions {
                        if opcodes::name(ins.opcode) != "callstatic" {
                            continue;
                        }
                        let (Some(Value::Identifier(c)), Some(Value::Identifier(n))) =
                            (ins.args.first(), ins.args.get(1))
                        else {
                            continue;
                        };
                        *by_class
                            .entry(f.text_or_empty(*c).to_ascii_lowercase())
                            .or_default()
                            .entry(f.text_or_empty(*n).to_ascii_lowercase())
                            .or_default() += 1;
                    }
                }
            }
        }
    }

    let mut rows: Vec<(String, usize, usize)> = by_class
        .iter()
        .map(|(c, fns)| (c.clone(), fns.values().sum(), fns.len()))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    println!("{} classes appear in callstatic\n", rows.len());
    println!("{:6} {:5}  class", "calls", "fns");
    for (c, calls, fns) in &rows {
        println!("{calls:6} {fns:5}  {c}");
    }

    println!("\nthe non-engine classes, function by function:");
    for (c, calls, _) in &rows {
        if matches!(c.as_str(), "game" | "debug" | "utility" | "math") {
            continue;
        }
        let fns = &by_class[c];
        let mut v: Vec<(&String, &usize)> = fns.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        let names: Vec<String> = v
            .iter()
            .take(12)
            .map(|(n, k)| format!("{n}×{k}"))
            .collect();
        println!("  {c}  ({calls} calls): {}", names.join(", "));
    }
}
