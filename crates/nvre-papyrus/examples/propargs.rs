//! What is the operand order of `propget` / `propset`?
//!
//! `showdiff` prints `propget CombatPreSet aScript ::temp5` where the emitter
//! writes `propget ::temp5 aScript CombatPreSet` — completely reversed. This
//! checks which slot overwhelmingly holds a `::tempN`, which is the destination,
//! so the order can be read off rather than assumed.
//!
//! It also revisits `proptags`, which counted the value tag at index 2 and found
//! `Identifier` 9,982 times: index 2 is a temporary, and a temporary is an
//! identifier too, so that count was consistent with BOTH orders and settled
//! nothing.

use nvre_papyrus::{PexFile, Value};
use std::collections::BTreeMap;

fn is_temp(s: &str) -> bool {
    s.starts_with("::temp") || s == "::NoneVar"
}

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

    // (op, slot) -> (times it holds a temp, total)
    let mut slots: BTreeMap<(&'static str, usize), (usize, usize)> = BTreeMap::new();
    let mut sample: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();

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
                    for (i, v) in ins.args.iter().enumerate() {
                        let text = match v {
                            Value::Identifier(x) => g.text_or_empty(*x).to_string(),
                            Value::String(x) => format!("{:?}", g.text_or_empty(*x)),
                            other => format!("{other:?}"),
                        };
                        let e = slots.entry((op, i)).or_insert((0, 0));
                        e.1 += 1;
                        if is_temp(&text) {
                            e.0 += 1;
                        }
                        if sample.entry(op).or_default().len() < 3 {
                            sample.entry(op).or_default().push(format!("{}={text}", i));
                        }
                    }
                }
            }
        }
    }

    for ((op, i), (temps, total)) in &slots {
        println!("{op:8} slot {i}: temp-like {temps}/{total}");
    }
    println!();
    for (op, ex) in &sample {
        println!("{op}: {}", ex.join("  "));
    }
}
