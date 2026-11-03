//! How many functions, not scripts, come out instruction-identical.
//!
//! `failclass` counts a *script* as differing if any one of its functions does,
//! so one bad function hides every good one beside it. This counts functions.

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
use std::collections::BTreeMap;
use std::io::Read;

fn operand(p: &PexFile, mine: Option<&[String]>, v: &Value) -> String {
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

fn text_of(p: &PexFile, mine: Option<&[String]>, f: &Function) -> Vec<String> {
    f.instructions
        .iter()
        .map(|i| {
            // `args` alone is NOT the whole instruction: `callmethod`,
            // `callstatic` and `callparent` carry every argument in
            // `varargs`. Omitting them made two call shapes compare equal —
            // `RegisterForAnimationEvent(self, "Apex")` and
            // `RegisterForAnimationEvent(self, "reset")` looked identical, and
            // so did `play(receiver)`. Every parity number measured with this
            // probe before this fix was blind to arguments.
            let mut out = opcodes::name(i.opcode).to_string();
            let mut all: Vec<&Value> = i.args.iter().collect();
            all.extend(i.varargs.iter());
            for a in all {
                out.push(' ');
                out.push_str(&operand(p, mine, a));
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

    let (mut fn_same, mut fn_diff) = (0usize, 0usize);
    let (mut scripts_perfect, mut scripts_any) = (0usize, 0usize);
    let mut buckets: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, String> = BTreeMap::new();

    for name in &sources {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let shipped_path = format!("scripts\\{leaf}.pex");
        let src = {
            let mut f = zip.by_name(name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            nvre_util::cp1252::decode(&b)
        };
        let Ok(g) = archive
            .extract_by_path(&shipped_path)
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

        scripts_any += 1;
        let mut all_same = true;
        for gstate in g.objects.iter().flat_map(|o| o.states.iter()) {
            for gf in &gstate.functions {
                let gname = g.text_or_empty(gf.name).to_string();
                let mine = c.states.iter().flat_map(|(_, fs)| fs.iter()).find(|f| {
                    c.strings.get(f.name as usize).map(String::as_str) == Some(gname.as_str())
                });
                let Some(m) = mine else {
                    fn_diff += 1;
                    all_same = false;
                    continue;
                };
                let gt = text_of(&g, None, gf);
                let mt = text_of(&g, Some(&c.strings), m);
                if gt == mt {
                    fn_same += 1;
                    continue;
                }
                fn_diff += 1;
                all_same = false;
                let key = match gt.iter().zip(&mt).position(|(x, y)| x != y) {
                    Some(n) => {
                        let shape = |v: &str| v.split(' ').next().unwrap_or("?").to_string();
                        format!("{} where game has {}", shape(&mt[n]), shape(&gt[n]))
                    }
                    None => format!("length {} vs {}", mt.len(), gt.len()),
                };
                *buckets.entry(key.clone()).or_default() += 1;
                examples.entry(key).or_insert_with(|| leaf.clone());
            }
        }
        if all_same {
            scripts_perfect += 1;
        }
    }

    let total = fn_same + fn_diff;
    println!(
        "functions identical: {fn_same} / {total} = {:.1}%",
        100.0 * fn_same as f64 / total as f64
    );
    println!(
        "scripts identical:   {scripts_perfect} / {scripts_any} = {:.1}%",
        100.0 * scripts_perfect as f64 / scripts_any as f64
    );
    println!();
    println!("first disagreement per function, most common first:");
    let mut v: Vec<_> = buckets.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, n) in v.iter().take(18) {
        println!(
            "{n:6}  {k}   e.g. {}",
            examples.get(k).map(String::as_str).unwrap_or("")
        );
    }
}
