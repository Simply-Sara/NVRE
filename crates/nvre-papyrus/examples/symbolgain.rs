//! Does filling the caller-supplied symbol table actually help?
//!
//! The claim behind `Symbols` is that a name inherited from a parent class types
//! as nothing when a script is compiled alone, and that supplying the parent
//! chain fixes it. That is testable: compile the corpus twice, once with an empty
//! `Symbols` (the old behaviour) and once with
//! [`Symbols::for_script`](nvre_papyrus::emit::Symbols::for_script) against a
//! [`ClassTable`](nvre_papyrus::symbols::ClassTable) built from every source.
//!
//! If the second run is better, the claim holds and the editor has a working
//! entry point. If it is worse, the fallback is misfiring and must not ship.
//!
//! ```text
//! cargo run --release --example symbolgain -- Scripts.zip "Misc.bsa"
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::Function;
use nvre_papyrus::symbols::ClassTable;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
use std::collections::BTreeMap;
use std::io::Read;

fn temp_index(name: &str) -> Option<i32> {
    name.strip_prefix("::temp")?.parse().ok()
}

fn base_of(p: &PexFile, mine: Option<&[String]>, f: &Function) -> i32 {
    let s = |i: u16| match mine {
        Some(t) => t.get(i as usize).cloned().unwrap_or_default(),
        None => p.text_or_empty(i).to_string(),
    };
    let mut low = i32::MAX;
    for i in &f.instructions {
        for a in i.args.iter().chain(i.varargs.iter()) {
            if let Value::Identifier(x) = a {
                if let Some(k) = temp_index(&s(*x)) {
                    low = low.min(k);
                }
            }
        }
    }
    if low == i32::MAX {
        0
    } else {
        low
    }
}

/// The instruction stream with temporaries renumbered relative to `base` and
/// jump offsets elided, so only a real structural difference remains.
fn canon(p: &PexFile, mine: Option<&[String]>, f: &Function, base: i32) -> Vec<Vec<String>> {
    let s = |i: u16| match mine {
        Some(t) => t.get(i as usize).cloned().unwrap_or_default(),
        None => p.text_or_empty(i).to_string(),
    };
    f.instructions
        .iter()
        .map(|i| {
            let name = opcodes::name(i.opcode);
            let mut out = vec![name.to_string()];
            let is_jump = matches!(name, "jmp" | "jmpt" | "jmpf");
            for (n, a) in i.args.iter().chain(i.varargs.iter()).enumerate() {
                if is_jump && n == 1 {
                    out.push("<off>".to_string());
                    continue;
                }
                out.push(match a {
                    Value::None => "None".to_string(),
                    Value::Identifier(x) => {
                        let t = s(*x);
                        match temp_index(&t) {
                            Some(k) => format!("::temp{}", k - base),
                            None => t,
                        }
                    }
                    Value::String(x) => format!("{:?}", s(*x)),
                    Value::Int(x) => x.to_string(),
                    Value::Float(x) => format!("{x}"),
                    Value::Bool(x) => x.to_string(),
                });
            }
            out
        })
        .collect()
}

/// The function in `state` named `gname`, paired by state AND name.
fn lookup<'a>(
    c: &'a nvre_papyrus::emit::CompiledScript,
    state: &str,
    name: &str,
) -> Option<&'a Function> {
    c.states
        .iter()
        .find(|(s, _)| {
            c.strings
                .get(*s as usize)
                .map(|n| n.to_ascii_lowercase() == state)
                .unwrap_or(false)
        })
        .and_then(|(_, fs)| {
            fs.iter()
                .find(|f| c.strings.get(f.name as usize).map(String::as_str) == Some(name))
        })
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

    // Parse every source ONCE. The table is built from all of them, which is what
    // an editor does when it compiles a mod.
    let mut parsed = Vec::new();
    for name in &sources {
        let src = {
            let mut f = zip.by_name(name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            nvre_util::cp1252::decode(&b)
        };
        if let Ok(s) = parse(&lex(&src)) {
            parsed.push((name.clone(), s));
        }
    }
    let table = ClassTable::new(parsed.iter().map(|(_, s)| s));
    println!("classes in the table: {}", table.names().count());

    let mut total = 0usize;
    let mut empty_ok = 0usize;
    let mut filled_ok = 0usize;
    let mut improved = 0usize;
    let mut worsened = 0usize;
    let mut examples: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();

    for (name, script) in &parsed {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let Ok(g) = archive
            .extract_by_path(&format!("scripts\\{leaf}.pex"))
            .map_err(|e| e.to_string())
            .and_then(|b| PexFile::parse(&b).map_err(|e| e.to_string()))
        else {
            continue;
        };

        let empty = Symbols {
            script: script.name.clone(),
            parent: script.parent.clone().unwrap_or_default(),
            variables: Vec::new(),
            properties: Vec::new(),
            states: Vec::new(),
        };
        let Ok(c_empty) = Compiler::new(script, empty).compile() else {
            continue;
        };
        let Ok(c_filled) = Compiler::new(script, Symbols::for_script(script, &table)).compile()
        else {
            continue;
        };

        for gstate in g.objects.iter().flat_map(|o| o.states.iter()) {
            let want_state = g.text_or_empty(gstate.name).to_ascii_lowercase();
            for gf in &gstate.functions {
                let gname = g.text_or_empty(gf.name).to_string();
                let m_empty = lookup(&c_empty, &want_state, &gname);
                let m_filled = lookup(&c_filled, &want_state, &gname);
                let (Some(m_empty), Some(m_filled)) = (m_empty, m_filled) else {
                    continue;
                };
                total += 1;

                let gt = canon(&g, None, gf, base_of(&g, None, gf));
                let te = canon(
                    &g,
                    Some(&c_empty.strings),
                    m_empty,
                    base_of(&g, Some(&c_empty.strings), m_empty),
                );
                let tf = canon(
                    &g,
                    Some(&c_filled.strings),
                    m_filled,
                    base_of(&g, Some(&c_filled.strings), m_filled),
                );
                let ok_empty = gt == te;
                let ok_filled = gt == tf;
                if ok_empty {
                    empty_ok += 1;
                }
                if ok_filled {
                    filled_ok += 1;
                }
                match (ok_empty, ok_filled) {
                    (false, true) => {
                        improved += 1;
                        let e = examples.entry("fixed by the symbol table").or_default();
                        if e.len() < 6 {
                            e.push(format!("{leaf}::{gname}"));
                        }
                    }
                    (true, false) => {
                        worsened += 1;
                        let e = examples.entry("BROKEN by the symbol table").or_default();
                        if e.len() < 6 {
                            e.push(format!("{leaf}::{gname}"));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    println!("functions paired: {total}");
    println!(
        "structurally identical, empty Symbols:  {empty_ok} ({:.2}%)",
        100.0 * empty_ok as f64 / total as f64
    );
    println!(
        "structurally identical, filled Symbols: {filled_ok} ({:.2}%)",
        100.0 * filled_ok as f64 / total as f64
    );
    println!("  improved by the symbol table: {improved}");
    println!("  BROKEN by the symbol table:   {worsened}");
    for (k, v) in &examples {
        println!("{k}:");
        for e in v {
            println!("   {e}");
        }
    }
}
