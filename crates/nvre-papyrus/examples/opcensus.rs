//! How often does each call opcode appear in the shipped corpus, and what does a
//! `callparent` look like?
//!
//! ```text
//! cargo run --release --example opcensus -- "Misc.bsa"
//! ```

use nvre_papyrus::{opcodes, PexFile, Value};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut archive = bsa105::Archive::open(&a[1]).unwrap();

    let mut counts: std::collections::BTreeMap<&'static str, usize> = Default::default();
    let mut shown = 0;
    let all: Vec<String> = archive.entries().map(|e| e.path()).collect();
    for path in all {
        let lower = path.to_ascii_lowercase();
        if !lower.ends_with(".pex") {
            continue;
        }
        let Ok(bytes) = archive.extract_by_path(&path) else {
            continue;
        };
        let Ok(p) = PexFile::parse(&bytes) else {
            continue;
        };
        for o in &p.objects {
            for st in &o.states {
                for f in &st.functions {
                    for ins in &f.instructions {
                        let n = opcodes::name(ins.opcode);
                        if matches!(n, "callmethod" | "callparent" | "callstatic") {
                            *counts.entry(n).or_default() += 1;
                            if n == "callparent" && shown < 8 {
                                shown += 1;
                                let mut s = String::new();
                                for v in ins.args.iter().chain(&ins.varargs) {
                                    s.push(' ');
                                    s.push_str(&match v {
                                        Value::None => "None".into(),
                                        Value::Identifier(i) => p.text_or_empty(*i).to_string(),
                                        Value::String(i) => format!("{:?}", p.text_or_empty(*i)),
                                        Value::Int(i) => i.to_string(),
                                        Value::Float(f) => f.to_string(),
                                        Value::Bool(b) => b.to_string(),
                                    });
                                }
                                println!(
                                    "callparent{}  [args {} varargs {}]",
                                    s,
                                    ins.args.len(),
                                    ins.varargs.len()
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    println!();
    for (k, v) in &counts {
        println!("{k:12} {v}");
    }
}
