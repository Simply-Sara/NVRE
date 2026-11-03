//! Split the remaining failures by *kind of* difference.
//!
//! Everything so far has counted "functions that differ". That hides the most
//! important distinction: a function whose instruction shapes and operands are
//! all correct but whose `::tempN` numbers drifted is a completely different
//! problem from one that emits the wrong instructions. This normalises every
//! temporary to a single token and compares again, so the two populations can be
//! separated and ordered by what they would cost to fix.
//!
//! ```text
//! cargo run --release --example failuremodes -- Scripts.zip "Misc.bsa"
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
use std::collections::BTreeMap;
use std::io::Read;

fn text(p: &PexFile, mine: Option<&[String]>, v: &Value, norm: bool) -> String {
    let s = |i: u16| match mine {
        Some(t) => t.get(i as usize).cloned().unwrap_or_default(),
        None => p.text_or_empty(i).to_string(),
    };
    match v {
        Value::None => "None".into(),
        Value::Identifier(i) => {
            let n = s(*i);
            // `::tempN` all collapse to one token: the numbering is the thing
            // being factored out.
            if norm && n.starts_with("::temp") && n[6..].parse::<u32>().is_ok() {
                "::temp".into()
            } else {
                n
            }
        }
        Value::String(i) => format!("{:?}", s(*i)),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Bool(b) => b.to_string(),
    }
}

fn lines(p: &PexFile, mine: Option<&[String]>, f: &Function, norm: bool) -> Vec<String> {
    f.instructions
        .iter()
        .map(|i| {
            let mut out = opcodes::name(i.opcode).to_string();
            for a in i.args.iter().chain(i.varargs.iter()) {
                out.push(' ');
                out.push_str(&text(p, mine, a, norm));
            }
            out
        })
        .collect()
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
    sources.sort();

    let mut modes: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut by_first_op: BTreeMap<String, usize> = BTreeMap::new();
    let mut example: BTreeMap<String, String> = BTreeMap::new();
    let mut exact = 0usize;
    let mut total = 0usize;

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
                total += 1;
                let Some(m) = mine else {
                    *modes.entry("function MISSING from our output").or_default() += 1;
                    continue;
                };
                let gt = lines(&g, None, gf, false);
                let mt = lines(&g, Some(&c.strings), m, false);
                if gt == mt {
                    exact += 1;
                    continue;
                }
                // Is it *only* the temporary numbers?
                let gt = lines(&g, None, gf, true);
                let mt = lines(&g, Some(&c.strings), m, true);
                if gt == mt {
                    *modes.entry("only ::tempN numbering").or_default() += 1;
                    continue;
                }
                // Otherwise: what is the first thing that differs?
                let k = match gt.iter().zip(&mt).position(|(x, y)| x != y) {
                    Some(n) => {
                        let shape = |v: &str| v.split(' ').next().unwrap_or("?").to_string();
                        if gt.len() != mt.len() {
                            // length differs, but not purely by numbering
                            format!("SHAPE {} vs {}", shape(&mt[n]), shape(&gt[n]))
                        } else {
                            let gs: Vec<&str> = gt[n].split(' ').collect();
                            let ms: Vec<&str> = mt[n].split(' ').collect();
                            if gs[0] != ms[0] {
                                format!("differs at instruction {}", n)
                            } else if gs.len() != ms.len() {
                                format!("operand COUNT on {}", gs[0])
                            } else {
                                format!("operand value on {}", gs[0])
                            }
                        }
                    }
                    None => format!("length {} vs {}", mt.len(), gt.len()),
                };
                *by_first_op.entry(k.clone()).or_default() += 1;
                example
                    .entry(k)
                    .or_insert_with(|| format!("{leaf}::{gname}"));
            }
        }
    }

    println!(
        "functions: {total}   instruction-exact: {exact} ({:.1}%)\n",
        100.0 * exact as f64 / total as f64
    );
    println!("the remaining failures, by kind:");
    for (k, n) in &modes {
        println!(
            "  {n:6}  {k}   ({:.1}% of failures)",
            100.0 * *n as f64 / (total - exact) as f64
        );
    }
    println!();
    println!("of the non-numbering failures, the first disagreement:");
    let mut v: Vec<_> = by_first_op.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    for (k, n) in v.iter().take(14) {
        println!("  {n:6}  {k}");
        if let Some(e) = example.get(*k) {
            println!("          e.g. {e}");
        }
    }
}
