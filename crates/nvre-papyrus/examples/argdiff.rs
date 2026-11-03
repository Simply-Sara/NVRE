//! What exactly differs inside a `callmethod`-vs-`callmethod` disagreement?
//!
//! `fncensus` buckets by mnemonics, which says "same instruction, wrong
//! operands" and stops there. This goes one level deeper: for every differing
//! `callmethod`, record which operand position first differs and what KIND of
//! difference it is — a different temporary number, a different name, a
//! different argument count. The answer decides what to fix next.
//!
//! ```text
//! cargo run --release --example argdiff -- Scripts.zip "Misc.bsa"
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
use std::collections::BTreeMap;
use std::io::Read;

fn text(p: &PexFile, mine: Option<&[String]>, v: &Value) -> String {
    let s = |i: u16| match mine {
        Some(t) => t.get(i as usize).cloned().unwrap_or_default(),
        None => p.text_or_empty(i).to_string(),
    };
    match v {
        Value::None => "None".into(),
        Value::Identifier(i) => s(*i),
        Value::String(i) => format!("{:?}", s(*i)),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Bool(b) => b.to_string(),
    }
}

fn operands(p: &PexFile, mine: Option<&[String]>, f: &Function) -> Vec<Vec<String>> {
    f.instructions
        .iter()
        .map(|i| {
            let mut out = vec![opcodes::name(i.opcode).to_string()];
            for a in i.args.iter().chain(i.varargs.iter()) {
                out.push(text(p, mine, a));
            }
            out
        })
        .collect()
}

/// `::temp7` -> `Some(7)`, so "same shape, different number" is visible.
fn temp_no(s: &str) -> Option<u32> {
    s.strip_prefix("::temp")?.parse().ok()
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

    let mut kinds: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut slots: BTreeMap<usize, usize> = BTreeMap::new();
    let mut examples: BTreeMap<&'static str, String> = BTreeMap::new();
    let mut delta: BTreeMap<i64, usize> = BTreeMap::new();

    for name in &sources {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let src = {
            let mut f = zip.by_name(name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            nvre_util::cp1252::decode(&b)
        };
        let Ok(g) = archive
            .extract_by_path(&format!("scripts\\{leaf}.pex"))
            .map_err(|e| e.to_string())
            .and_then(|b| PexFile::parse(&b).map_err(|e| e.to_string()))
        else {
            continue;
        };
        let Ok(script) = parse(&lex(&src)) else {
            continue;
        };
        let symbols = Symbols {
            script: script.name.clone(),
            parent: script.parent.clone().unwrap_or_default(),
            variables: Vec::new(),
            properties: Vec::new(),
            states: Vec::new(),
        };
        let Ok(c) = Compiler::new(&script, symbols).compile() else {
            continue;
        };

        for gstate in g.objects.iter().flat_map(|o| o.states.iter()) {
            for gf in &gstate.functions {
                let gname = g.text_or_empty(gf.name).to_string();
                let mine = c.states.iter().flat_map(|(_, fs)| fs.iter()).find(|f| {
                    c.strings.get(f.name as usize).map(String::as_str) == Some(gname.as_str())
                });
                let Some(m) = mine else { continue };
                let gt = operands(&g, None, gf);
                let mt = operands(&g, Some(&c.strings), m);
                if gt == mt {
                    continue;
                }
                for (gi, mi) in gt.iter().zip(&mt) {
                    if gi == mi {
                        continue;
                    }
                    if gi[0] != "callmethod" || mi[0] != "callmethod" {
                        continue;
                    }
                    if gi.len() != mi.len() {
                        *kinds.entry("argument COUNT differs").or_default() += 1;
                        examples
                            .entry("argument COUNT differs")
                            .or_insert_with(|| format!("{leaf}::{gname}"));
                        continue;
                    }
                    let Some(pos) = gi.iter().zip(mi).position(|(x, y)| x != y) else {
                        continue;
                    };
                    *slots.entry(pos).or_default() += 1;
                    let k = match (temp_no(&gi[pos]), temp_no(&mi[pos])) {
                        (Some(x), Some(y)) => {
                            *delta.entry(y as i64 - x as i64).or_default() += 1;
                            "same name, different ::tempN"
                        }
                        _ => "different name entirely",
                    };
                    *kinds.entry(k).or_default() += 1;
                    examples.entry(k).or_insert_with(|| {
                        format!("{leaf}::{gname} [{pos}] {} vs {}", mi[pos], gi[pos])
                    });
                }
            }
        }
    }

    println!("how a differing callmethod differs:");
    for (k, n) in &kinds {
        println!("{n:6}  {k}");
        println!(
            "        e.g. {}",
            examples.get(k).map(String::as_str).unwrap_or("")
        );
    }
    println!();
    println!("first differing operand position (0=method, 1=receiver, 2=dest, 3+=args):");
    for (p, n) in &slots {
        println!("  {p}: {n}");
    }
    println!();
    println!("mine minus game temp number, when both are ::tempN:");
    for (d, n) in &delta {
        println!("  {d:+}: {n}");
    }
}
