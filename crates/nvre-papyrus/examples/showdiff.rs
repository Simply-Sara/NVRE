//! Show the first disagreement in full, with arguments, for one bucket.
//!
//! `failclass` and `fncensus` bucket differences by mnemonics, which was enough
//! to find classes of bug but not enough to see what is wrong inside the
//! biggest one. This prints the actual instruction pair, operands and all, plus
//! a few real examples so a pattern can be read off rather than guessed.
//!
//! ```text
//! cargo run --release --example showdiff -- Scripts.zip "Misc.bsa" callmethod:callmethod 12
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
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
            let mut out = opcodes::name(i.opcode).to_string();
            for a in i.args.iter().chain(i.varargs.iter()) {
                out.push(' ');
                out.push_str(&operand(p, mine, a));
            }
            out
        })
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let want = a.get(3).cloned().unwrap_or_default();
    let limit: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(10);

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

    let mut shown = 0usize;
    for name in &sources {
        if shown >= limit {
            break;
        }
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
                let gt = text_of(&g, None, gf);
                let mt = text_of(&g, Some(&c.strings), m);
                if gt == mt {
                    continue;
                }
                // Which bucket is this in?
                let key = match gt.iter().zip(&mt).position(|(x, y)| x != y) {
                    Some(n) => {
                        let shape = |v: &str| v.split(' ').next().unwrap_or("?").to_string();
                        format!("{}:{}", shape(&mt[n]), shape(&gt[n]))
                    }
                    None => "length".to_string(),
                };
                if !want.is_empty() && key != want {
                    continue;
                }
                println!("=== {leaf} :: {}", gname);
                let at = gt.iter().zip(&mt).position(|(x, y)| x != y).unwrap_or(0);
                let lo = at.saturating_sub(2);
                for i in lo..gt.len().max(mt.len()) {
                    let gi = gt.get(i).map(String::as_str).unwrap_or("<absent>");
                    let mi = mt.get(i).map(String::as_str).unwrap_or("<absent>");
                    let mark = if gi == mi { "  " } else { "!!" };
                    println!("{mark}{i:3} game {gi}");
                    println!("    {i:3} mine {mi}");
                }
                println!();
                shown += 1;
                break;
            }
        }
    }
}
