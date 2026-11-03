//! What the injected `GetState` / `GotoState` actually look like across the
//! corpus, and where they live.
//!
//! Two questions, both answerable only from the shipped bytes:
//!   1. Which states carry the injected pair — the default state only, or every
//!      state?
//!   2. Does `GetState` return `::State`, or the state's name as a literal?

use nvre_papyrus::{opcodes, PexFile, Value};
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&a[1]).unwrap()).unwrap();
    let mut archive = bsa105::Archive::open(&a[2]).unwrap();

    let mut sources = Vec::new();
    for i in 0..zip.len() {
        let name = zip.by_index(i).unwrap().name().to_string();
        if name.ends_with(".psc") {
            sources.push(name);
        }
    }
    sources.sort();

    // Where does the pair appear? keyed by "default state" / "named state".
    let mut where_: BTreeMap<&'static str, usize> = BTreeMap::new();
    // What does GetState return?
    let mut ret_shape: BTreeMap<String, usize> = BTreeMap::new();
    let mut ret_example: BTreeMap<String, String> = BTreeMap::new();
    // How many named states does a script have, split by whether the default
    // state exists at all?
    let mut shape: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut n = 0usize;
    let mut pos: BTreeMap<&'static str, usize> = BTreeMap::new();

    for name in &sources {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let Ok(bytes) = archive.extract_by_path(&format!("scripts\\{leaf}.pex")) else {
            continue;
        };
        let Ok(g) = PexFile::parse(&bytes) else {
            continue;
        };
        n += 1;
        let Some(o) = g.objects.first() else { continue };

        let has_default = o.states.iter().any(|s| g.text_or_empty(s.name).is_empty());
        if o.states.len() > 1 {
            let first = g.text_or_empty(o.states[0].name).is_empty();
            *pos.entry(if first {
                "default is first"
            } else {
                "default is NOT first"
            })
            .or_default() += 1;
        }
        let named = o
            .states
            .iter()
            .filter(|s| !g.text_or_empty(s.name).is_empty())
            .count();
        let key = match (has_default, named) {
            (true, 0) => "default only",
            (true, _) => "default + named",
            (false, _) => "named only (NO default state)",
        };
        *shape.entry(key).or_default() += 1;

        for st in &o.states {
            let sname = g.text_or_empty(st.name).to_string();
            let is_default = sname.is_empty();
            for f in &st.functions {
                let fname = g.text_or_empty(f.name);
                if fname != "GetState" && fname != "GotoState" {
                    continue;
                }
                let k = if is_default {
                    if fname == "GetState" {
                        "GetState in default"
                    } else {
                        "GotoState in default"
                    }
                } else if fname == "GetState" {
                    "GetState in NAMED state"
                } else {
                    "GotoState in NAMED state"
                };
                *where_.entry(k).or_default() += 1;

                if fname == "GetState" {
                    // The operand of its single `return`.
                    let shape = match f.instructions.first().and_then(|i| i.args.first()) {
                        Some(Value::Identifier(i)) => {
                            if g.text_or_empty(*i) == "::State" {
                                "return ::State".to_string()
                            } else {
                                format!("return ident {}", g.text_or_empty(*i))
                            }
                        }
                        Some(Value::String(i)) => {
                            format!("return literal {:?}", g.text_or_empty(*i))
                        }
                        _ => {
                            let ops: Vec<&str> = f
                                .instructions
                                .iter()
                                .map(|i| opcodes::name(i.opcode))
                                .collect();
                            format!("other: {}", ops.join(","))
                        }
                    };
                    *ret_shape.entry(shape.clone()).or_default() += 1;
                    ret_example.entry(shape).or_insert_with(|| leaf.clone());
                }
            }
        }
    }

    println!("scripts scanned: {n}\n");
    println!("position of the default state, multi-state scripts only:");
    for (k, v) in &pos {
        println!("  {v:6}  {k}");
    }
    println!();
    println!("state layout:");
    for (k, v) in &shape {
        println!("  {v:6}  {k}");
    }
    println!("\nwhere the injected pair appears:");
    for (k, v) in &where_ {
        println!("  {v:6}  {k}");
    }
    println!("\nGetState return shape:");
    for (k, v) in &ret_shape {
        println!("  {v:6}  {k}   e.g. {}", ret_example.get(k).unwrap());
    }
}
