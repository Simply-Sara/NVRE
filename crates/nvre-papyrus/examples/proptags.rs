//! What value tag the property name carries in `propget` / `propset`.
//!
//! The emitter writes it as `Value::String`, which produces
//! `propget IsBluntedQuest ::temp3 ::temp4` against the game's
//! `propget IsBluntedQuest ::temp3 ::temp4` — same text, different TAG, and the
//! tag is a byte in the file. This counts the tags the shipped corpus actually
//! uses so the choice is measured rather than assumed.

use nvre_papyrus::{PexFile, Value};
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

    let mut tags: BTreeMap<(&'static str, &'static str, u8), usize> = BTreeMap::new();
    let mut examples: BTreeMap<(&'static str, &'static str, u8), String> = BTreeMap::new();

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
        for st in g.objects.iter().flat_map(|o| o.states.iter()) {
            for f in &st.functions {
                for ins in &f.instructions {
                    let op = nvre_papyrus::opcodes::name(ins.opcode);
                    if op != "propget" && op != "propset" {
                        continue;
                    }
                    // The property name is the third operand of propget
                    // (dest, object, name) and the second of propset
                    // (object, name, value).
                    let idx = if op == "propget" { 2 } else { 1 };
                    let Some(v) = ins.args.get(idx) else { continue };
                    let tag = match v {
                        Value::None => "None",
                        Value::Identifier(_) => "Identifier",
                        Value::String(_) => "String",
                        Value::Int(_) => "Int",
                        Value::Float(_) => "Float",
                        Value::Bool(_) => "Bool",
                    };
                    let k = (op, tag, v.tag());
                    *tags.entry(k).or_default() += 1;
                    examples.entry(k).or_insert_with(|| leaf.clone());
                }
            }
        }
    }

    for ((op, tag, t), n) in &tags {
        println!(
            "{op:8} name is {tag:11} (tag byte {t})  x{n}   e.g. {}",
            examples.get(&(*op, *tag, *t)).unwrap()
        );
    }
}
