//! Probe: the exact spelling of a `callstatic` class operand, per class.
//!
//! ```text
//! cargo run --release --example classspelling -- "Skyrim - Misc.bsa"
//! ```
//!
//! The `.pex` string table keeps the **first** spelling it saw for a name, so a
//! class operand can come back capitalised or not depending on what else the
//! file interned earlier: `callstatic CWScript log` in one script, `callstatic
//! sound stopinstance` in another. This counts the spellings, because picking
//! one for the emitter is only defensible if the corpus is lopsided.
use nvre_papyrus::{opcodes, PexFile, Value};
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 2 {
        eprintln!("usage: classspelling <archive.bsa>");
        return;
    }
    let mut archive = bsa105::Archive::open(&a[1]).unwrap();
    let paths: Vec<String> = archive
        .entries()
        .map(|e| e.path())
        .filter(|p| p.to_ascii_lowercase().ends_with(".pex"))
        .collect();

    // lowercased class -> spelling -> count
    let mut spellings: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
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
                        let Some(Value::Identifier(c)) = ins.args.first() else {
                            continue;
                        };
                        let text = f.text_or_empty(*c).to_string();
                        *spellings
                            .entry(text.to_ascii_lowercase())
                            .or_default()
                            .entry(text)
                            .or_default() += 1;
                    }
                }
            }
        }
    }

    for (lower, by_spelling) in &spellings {
        let total: usize = by_spelling.values().sum();
        let mut v: Vec<(&String, &usize)> = by_spelling.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        let shown: Vec<String> = v.iter().take(6).map(|(s, n)| format!("{s:?}×{n}")).collect();
        println!("{lower:24} {total:5}  {}", shown.join(", "));
    }
}
