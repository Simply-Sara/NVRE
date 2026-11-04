//! Probe: which script emits each non-engine `callstatic`, and how it spells
//! the class operand.
//!
//! ```text
//! cargo run --release --example callstaticwho -- "Skyrim - Misc.bsa" cwscript
//! ```
//!
//! The class operand's capitalisation is not fixed: `callstatic CWScript log`
//! but `callstatic sound stopinstance` in the same corpus. Before choosing a
//! spelling for the emitter, this prints the caller beside the operand and the
//! caller's own object name, which is the string interned first in the file and
//! therefore the candidate explanation.
use nvre_papyrus::{opcodes, PexFile, Value};
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: callstaticwho <archive.bsa> <class>");
        return;
    }
    let want = a[2].to_ascii_lowercase();
    let mut archive = bsa105::Archive::open(&a[1]).unwrap();
    let paths: Vec<String> = archive
        .entries()
        .map(|e| e.path())
        .filter(|p| p.to_ascii_lowercase().ends_with(".pex"))
        .collect();

    // (operand spelling, caller object, caller's own name as interned)
    let mut rows: BTreeMap<String, usize> = BTreeMap::new();
    for path in &paths {
        let Ok(bytes) = archive.extract_by_path(path) else {
            continue;
        };
        let Ok(f) = PexFile::parse(&bytes) else {
            continue;
        };
        for obj in &f.objects {
            let obj_name = f.text_or_empty(obj.name).to_string();
            for st in &obj.states {
                for func in &st.functions {
                    for ins in &func.instructions {
                        if opcodes::name(ins.opcode) != "callstatic" {
                            continue;
                        }
                        let Some(Value::Identifier(c)) = ins.args.first() else {
                            continue;
                        };
                        let text = f.text_or_empty(*c);
                        if text.to_ascii_lowercase() != want {
                            continue;
                        }
                        *rows
                            .entry(format!("operand {text:?}  caller {obj_name}"))
                            .or_default() += 1;
                    }
                }
            }
        }
    }
    let mut v: Vec<(&String, &usize)> = rows.iter().collect();
    v.sort_by(|a, b| a.0.cmp(b.0));
    for (k, n) in v {
        println!("{n:5}  {k}");
    }
}
