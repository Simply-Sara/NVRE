//! What is actually still wrong, bucketed by the first instruction that differs.
//!
//! `contentcensus` says 9,502 scripts disagree on states and functions. That is
//! a count, not a diagnosis. This walks those scripts and reports the first
//! disagreement as a pair of mnemonics, so the remaining work is a list of
//! classes rather than a number.
//!
//! Run with the SE corpus:
//!
//! ```text
//! cargo run --release --example failclass -- Scripts.zip "Skyrim - Misc.bsa"
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
use std::collections::BTreeMap;
use std::io::Read;

/// Turn one operand into comparable text, resolving identifiers through the
/// string table it belongs to. Two compilers numbering their tables differently
/// still agree on the words.
fn operand(p: &PexFile, tidx: Option<&[String]>, v: &Value) -> String {
    let s = |i: u16| match tidx {
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

fn instr_text(p: &PexFile, t: Option<&[String]>, i: &nvre_papyrus::Instruction) -> String {
    let mut out = opcodes::name(i.opcode).to_string();
    for a in &i.args {
        out.push(' ');
        out.push_str(&operand(p, t, a));
    }
    out
}

/// A function's instructions as text, for comparison.
fn text_of(p: &PexFile, t: Option<&[String]>, f: &Function) -> Vec<String> {
    f.instructions
        .iter()
        .enumerate()
        .map(|(n, i)| format!("{n}: {}", instr_text(p, t, i)))
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: failclass <Scripts.zip> <archive>");
        return;
    }

    // Sources, by lowercased leaf with no extension, the way the archives key them.
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&a[1]).unwrap()).unwrap();
    let mut archive = bsa105::Archive::open(&a[2]).unwrap();

    // Sources, in a stable order, keyed the way the archive keys them.
    let mut sources = Vec::new();
    for i in 0..zip.len() {
        let name = zip.by_index(i).unwrap().name().to_string();
        if name.ends_with(".psc") {
            sources.push(name);
        }
    }
    sources.sort();
    let mut buckets: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, String> = BTreeMap::new();
    let (mut same, mut differ, mut failed) = (0usize, 0usize, 0usize);

    for name in &sources {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let shipped_path = format!("scripts\\{leaf}.pex");
        let src_bytes = {
            let mut f = zip.by_name(name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            b
        };
        let s = &nvre_util::cp1252::decode(&src_bytes);
        let Ok(game) = archive
            .extract_by_path(&shipped_path)
            .map_err(|e| e.to_string())
            .and_then(|b| PexFile::parse(&b).map_err(|e| e.to_string()))
        else {
            continue;
        };
        let game = &game;
        let Ok(script) = parse(&lex(s)) else {
            failed += 1;
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
            failed += 1;
            continue;
        };
        let g = game;

        // Compare every function present on both sides, by name.
        let mut worst: Option<String> = None;
        for gstate in g.objects.iter().flat_map(|o| o.states.iter()) {
            for gf in &gstate.functions {
                let gname = g.text_or_empty(gf.name);
                let mine =
                    c.states.iter().flat_map(|(_, fs)| fs.iter()).find(|f| {
                        c.strings.get(f.name as usize).map(String::as_str) == Some(gname)
                    });
                let Some(m) = mine else {
                    worst.get_or_insert_with(|| format!("missing function {gname}"));
                    continue;
                };
                let gt = text_of(&g, None, gf);
                let mt = text_of(&g, Some(&c.strings), m);
                let strip = |v: &Vec<String>| {
                    v.iter()
                        .map(|x| x.split_once(": ").unwrap().1.to_string())
                        .collect::<Vec<_>>()
                };
                let (gt, mt) = (strip(&gt), strip(&mt));
                if gt == mt {
                    continue;
                }
                // First disagreement, as "mine => game".
                let at = gt.iter().zip(&mt).position(|(x, y)| x != y);
                let key = match at {
                    Some(n) => {
                        let (gv, mv) = (&gt[n], &mt[n]);
                        let shape = |v: &str| v.split(' ').next().unwrap_or("?").to_string();
                        format!("{} where game has {}", shape(mv), shape(gv))
                    }
                    None => format!("length {} vs {}", mt.len(), gt.len()),
                };
                worst.get_or_insert(key);
            }
        }

        match worst {
            None => same += 1,
            Some(k) => {
                *buckets.entry(k.clone()).or_default() += 1;
                examples.entry(k).or_insert_with(|| name.clone());
                differ += 1;
            }
        }
    }

    println!("functions identical: {same}");
    println!("differing:           {differ}");
    println!("failed to compile:   {failed}");
    println!();
    println!("first disagreement, most common first:");
    let mut v: Vec<_> = buckets.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, n) in v.iter().take(30) {
        println!("{n:6}  {k}");
        if let Some(e) = examples.get(k) {
            println!("        e.g. {e}");
        }
    }
}
