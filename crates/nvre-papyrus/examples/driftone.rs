//! Per-function temporary-counter offsets for one script, in compile order.
//!
//! `driftwho` names the first function that fails to align; this shows the
//! offsets leading up to it, which is how a function that *aligns* but still
//! advances the counter gets found. That is the shape a `release_temp` bug leaves
//! behind: the instructions are identical, the base is not.
//!
//! ```text
//! cargo run --release --example driftone -- Scripts.zip "Misc.bsa" objectreference
//! ```
//!
//! `k` is `mine base - game base`. `aligned` says whether the two streams match
//! once each is renumbered by its own base. A row with `k != 0` and
//! `aligned = true` is a function that emitted the right code and left the counter
//! in the wrong place — the culprit for everything below it.

#[path = "support/mod.rs"]
mod support;

use nvre_papyrus::emit::Compiler;
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};

fn temp_index(name: &str) -> Option<i32> {
    name.strip_prefix("::temp")?.parse().ok()
}

fn base_of(p: &PexFile, mine: Option<&[String]>, f: &Function) -> i32 {
    let s = |i: u16| match mine {
        Some(m) => m.get(i as usize).cloned().unwrap_or_default(),
        None => p.text_or_empty(i).to_string(),
    };
    let mut base = i32::MAX;
    for i in &f.instructions {
        for v in i.args.iter().chain(i.varargs.iter()) {
            if let Value::Identifier(x) = v {
                if let Some(n) = temp_index(&s(*x)) {
                    base = base.min(n);
                }
            }
        }
    }
    if base == i32::MAX {
        0
    } else {
        base
    }
}

fn canonical(p: &PexFile, mine: Option<&[String]>, f: &Function, base: i32) -> Vec<String> {
    let s = |i: u16| match mine {
        Some(m) => m.get(i as usize).cloned().unwrap_or_default(),
        None => p.text_or_empty(i).to_string(),
    };
    f.instructions
        .iter()
        .map(|i| {
            let args: Vec<String> = i
                .args
                .iter()
                .chain(i.varargs.iter())
                .map(|v| match v {
                    Value::Identifier(x) => {
                        let t = s(*x);
                        match temp_index(&t) {
                            Some(n) => format!("t{}", n - base),
                            None => t,
                        }
                    }
                    Value::String(x) => format!("{:?}", s(*x)),
                    Value::Int(n) => n.to_string(),
                    Value::Float(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::None => "None".to_string(),
                })
                .collect();
            format!("{} {}", opcodes::name(i.opcode), args.join(" "))
        })
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let want = a[3].to_ascii_lowercase();
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
    let table = support::class_table(&mut zip, &sources);

    let Some(src_name) = sources
        .iter()
        .find(|n| n.to_ascii_lowercase().ends_with(&format!("{want}.psc")))
        .cloned()
    else {
        eprintln!("no source for {want}");
        return;
    };
    let Some(src) = support::source_of(&mut zip, &src_name) else {
        return;
    };
    let Ok(script) = parse(&lex(&src)) else {
        eprintln!("parse failed");
        return;
    };
    let Ok(g) = archive
        .extract_by_path(&format!("scripts\\{want}.pex"))
        .map_err(|e| e.to_string())
        .and_then(|b| PexFile::parse(&b).map_err(|e| e.to_string()))
    else {
        eprintln!("no .pex");
        return;
    };
    let symbols = nvre_papyrus::emit::Symbols::for_script(&script, &table);
    let Ok(c) = Compiler::new(&script, symbols).compile() else {
        eprintln!("compile failed");
        return;
    };

    let mut pairs: Vec<(i32, &Function, &Function, String)> = Vec::new();
    for gstate in g.objects.iter().flat_map(|o| o.states.iter()) {
        let want_state = g.text_or_empty(gstate.name).to_ascii_lowercase();
        for gf in &gstate.functions {
            let gname = g.text_or_empty(gf.name).to_string();
            let mine = c
                .states
                .iter()
                .find(|(s, _)| {
                    c.strings
                        .get(*s as usize)
                        .map(|n| n.to_ascii_lowercase() == want_state)
                        .unwrap_or(false)
                })
                .and_then(|(_, fs)| {
                    fs.iter().find(|f| {
                        c.strings
                            .get(f.name as usize)
                            .map(|n| n.eq_ignore_ascii_case(&gname))
                            .unwrap_or(false)
                    })
                });
            let Some(m) = mine else { continue };
            pairs.push((base_of(&g, None, gf), gf, m, gname));
        }
    }
    pairs.sort_by_key(|(b, _, _, _)| *b);

    println!("{:>4} {:>4} {:>4}  {:<9} {}", "game", "mine", "k", "aligned", "function");
    for (gb, gf, m, gname) in &pairs {
        let mb = base_of(&g, Some(&c.strings), m);
        let aligned = canonical(&g, None, gf, *gb) == canonical(&g, Some(&c.strings), m, mb);
        println!(
            "{gb:>4} {mb:>4} {:>4}  {:<9} {}",
            mb - gb,
            if aligned { "yes" } else { "NO" },
            gname
        );
    }
}
