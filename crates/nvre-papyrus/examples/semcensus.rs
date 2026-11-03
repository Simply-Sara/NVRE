//! How much of the shipped corpus does the emitter reproduce, semantically?
//!
//! `parity` compares bytes, which cannot answer this: the Creation Kit orders
//! its string table differently, so every index differs even when the code is
//! the same. This compares the *program*: each operand is resolved through its
//! own file's table and compared as text, and functions are matched **by name**
//! rather than position, because the CK stores a state's functions in a hash
//! container and emits them in its iteration order.
//!
//! Usage: semcensus <Scripts.zip> <Skyrim - Misc.bsa>

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, PexFile, Value};
use std::collections::BTreeMap;
use std::io::Read;

fn instr(strings: &[String], i: &nvre_papyrus::pex::Instruction) -> String {
    let a: Vec<String> = i
        .args
        .iter()
        .map(|x| match x {
            Value::Identifier(k) => {
                format!(
                    "i({})",
                    strings.get(*k as usize).cloned().unwrap_or_default()
                )
            }
            Value::String(k) => format!(
                "s({})",
                strings.get(*k as usize).cloned().unwrap_or_default()
            ),
            o => format!("{o:?}"),
        })
        .collect();
    format!("{:?}[{}]", i.opcode, a.join(","))
}

fn locals(strings: &[String], f: &nvre_papyrus::pex::Function) -> Vec<String> {
    f.locals
        .iter()
        .map(|l| {
            format!(
                "L {} {}",
                strings.get(l.name as usize).cloned().unwrap_or_default(),
                strings
                    .get(l.type_name as usize)
                    .cloned()
                    .unwrap_or_default()
            )
        })
        .collect()
}

fn game_functions(f: &PexFile) -> BTreeMap<String, Vec<String>> {
    let mut m = BTreeMap::new();
    for o in &f.objects {
        for s in &o.states {
            for fun in &s.functions {
                let mut v = locals(&f.strings, fun);
                v.extend(fun.instructions.iter().map(|i| instr(&f.strings, i)));
                // Keyed by state as well as name: a script may declare
                // `onActivate` in the default state and again in a named one,
                // and they are different functions with different bodies.
                m.insert(key(f.text_or_empty(s.name), f.text_or_empty(fun.name)), v);
            }
        }
    }
    m
}

/// `state|function`, lowercased. The default state is the empty string.
fn key(state: &str, function: &str) -> String {
    format!(
        "{}|{}",
        state.to_ascii_lowercase(),
        function.to_ascii_lowercase()
    )
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&a[1]).unwrap()).unwrap();
    let mut archive = bsa105::Archive::open(&a[2]).unwrap();
    let shipped: Vec<String> = archive
        .entries()
        .map(|e| e.path())
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            l.starts_with("scripts\\") && l.ends_with(".pex")
        })
        .collect();
    let mut sources = Vec::new();
    for i in 0..zip.len() {
        let n = zip.by_index(i).unwrap().name().to_string();
        if n.to_ascii_lowercase().ends_with(".psc") {
            sources.push(n);
        }
    }
    sources.sort();

    let (mut same, mut diff, mut fail, mut skip) = (0usize, 0usize, 0usize, 0usize);
    let mut eq_lines = 0usize;
    let mut tot_lines = 0usize;
    let mut examples: Vec<String> = Vec::new();

    for name in &sources {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let shipped_path = format!("scripts\\{leaf}.pex");
        if !shipped
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&shipped_path))
        {
            skip += 1;
            continue;
        }
        let src = {
            let mut f = zip.by_name(name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            nvre_util::cp1252::decode(&b)
        };
        let game = match archive
            .extract_by_path(&shipped_path)
            .map_err(|e| e.to_string())
            .and_then(|b| PexFile::parse(&b).map_err(|e| e.to_string()))
        {
            Ok(g) => g,
            Err(_) => {
                fail += 1;
                continue;
            }
        };
        let script = match parse(&lex(&src)) {
            Ok(s) => s,
            Err(_) => {
                fail += 1;
                continue;
            }
        };
        let symbols = Symbols {
            script: script.name.clone(),
            parent: script.parent.clone().unwrap_or_default(),
            variables: Vec::new(),
            properties: Vec::new(),
            states: Vec::new(),
        };
        let c = match Compiler::new(&script, symbols).compile() {
            Ok(c) => c,
            Err(_) => {
                fail += 1;
                continue;
            }
        };

        let mut mine: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (sn, funcs) in &c.states {
            let state = c.strings.get(*sn as usize).cloned().unwrap_or_default();
            for f in funcs {
                let mut v = locals(&c.strings, f);
                v.extend(f.instructions.iter().map(|i| instr(&c.strings, i)));
                let name = c.strings.get(f.name as usize).cloned().unwrap_or_default();
                mine.insert(key(&state, &name), v);
            }
        }
        let theirs = game_functions(&game);

        for (k, v) in &mine {
            if let Some(o) = theirs.get(k) {
                tot_lines += v.len().max(o.len());
                eq_lines += v.iter().zip(o.iter()).filter(|(x, y)| x == y).count();
            }
        }
        if mine == theirs {
            same += 1;
        } else {
            diff += 1;
            if examples.len() < 8 {
                if let Some((k, v)) = mine
                    .iter()
                    .find(|(k, v)| theirs.get(*k).map(|o| o != *v).unwrap_or(true))
                {
                    let o = theirs.get(k).cloned().unwrap_or_default();
                    let x = v.iter().zip(o.iter()).find(|(p, q)| p != q);
                    examples.push(format!(
                        "{leaf} :: {k}: {}",
                        match x {
                            Some((p, q)) => format!("mine `{p}` game `{q}`"),
                            None => format!("length {} vs {}", v.len(), o.len()),
                        }
                    ));
                }
            }
        }
    }

    println!("functions identical in every respect: {same} scripts");
    println!("scripts that differ somewhere:           {diff}");
    println!("parse/emit/archive failures:             {fail}");
    println!("skipped (no shipped .pex):               {skip}");
    println!(
        "lines equal {eq_lines}/{tot_lines} = {:.3}",
        if tot_lines > 0 {
            eq_lines as f64 / tot_lines as f64
        } else {
            0.0
        }
    );
    for e in &examples {
        println!("  {e}");
    }
}
