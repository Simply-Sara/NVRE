//! Which function poisons a script's temporary counter?
//!
//! The counter of handed-out temporaries is **script-wide and monotonic**, so one
//! function that allocates one temporary too many (or too few) shifts every later
//! function in the same script. That is why a single defect can look like dozens
//! of unrelated numbering differences.
//!
//! `drift` reports the offset histogram and where drift first appears. This names
//! the **culprit**: for each script, the functions are ordered by their game base
//! (which recovers the compile order), and the probe prints the function whose own
//! stream does not align at any offset — the one the offset changes across. That
//! function is worth reading, because fixing it fixes every function after it in
//! the same file.
//!
//! ```text
//! cargo run --release --example driftwho -- Scripts.zip "Misc.bsa" [limit]
//! ```
//!
//! The count printed per culprit is `their locals - our locals`: negative means we
//! hand out more temporaries than the game, positive means fewer.

#[path = "support/mod.rs"]
mod support;

use nvre_papyrus::emit::Compiler;
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
use std::collections::BTreeMap;

/// A `::tempN` operand's index.
fn temp_index(name: &str) -> Option<i32> {
    name.strip_prefix("::temp")?.parse().ok()
}

/// The function's base: the lowest temporary it names.
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

/// The instruction stream with every temporary shifted by `-base`.
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
    let limit: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(25);
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

    // culprit name -> (#scripts it poisons, total functions after it, delta)
    let mut culprits: BTreeMap<String, (usize, usize, i32)> = BTreeMap::new();
    let mut scripts = 0usize;

    for name in &sources {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let Some(src) = support::source_of(&mut zip, name) else {
            continue;
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
        let symbols = nvre_papyrus::emit::Symbols::for_script(&script, &table);
        let Ok(c) = Compiler::new(&script, symbols).compile() else {
            continue;
        };

        // Pair by state + name, then order by the game's base: that is the
        // order the CK compiled them in, because the counter only grows.
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

        // Only the FIRST function in the script that fails to align at its own
        // base is named. Everything after it is displaced by that one, and
        // counting them all makes a single 400-function script look like 400
        // separate culprits — which is exactly the confusion this probe exists to
        // remove.
        let mut blamed = false;
        for (idx, (_b, gf, m, gname)) in pairs.iter().enumerate() {
            if blamed {
                break;
            }
            let gb = base_of(&g, None, gf);
            let mb = base_of(&g, Some(&c.strings), m);
            let aligned = canonical(&g, None, gf, gb) == canonical(&g, Some(&c.strings), m, mb);
            if !aligned {
                // `delta` = our locals minus the game's: positive means we hand
                // out fewer temporaries than the game, negative more.
                let delta = m.locals.len() as i32 - gf.locals.len() as i32;
                let after = pairs.len() - idx - 1;
                let e = culprits
                    .entry(format!("{leaf}::{gname}"))
                    .or_insert((0, 0, delta));
                e.0 += 1;
                e.1 += after;
                scripts += 1;
                blamed = true;
            }
        }
    }

    let mut rows: Vec<(&String, &(usize, usize, i32))> = culprits.iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1 .1));
    println!("scripts with a non-aligning function: {scripts}");
    println!("{} distinct culprits\n", rows.len());
    println!("{:5} {:5} {:6}  function", "files", "after", "delta");
    for (k, v) in rows.iter().take(limit) {
        println!("{:5} {:5} {:+6}  {k}", v.0, v.1, v.2);
    }
}
