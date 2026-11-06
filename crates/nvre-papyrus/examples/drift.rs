//! Where does the temporary counter drift, and what does it cost?
//!
//! The counter of handed-out temporaries is **script-wide and monotonic**, so one
//! temporary allocated too many in an early function shifts every later function
//! in the same script. That makes a single small bug look like hundreds of
//! unrelated numbering differences.
//!
//! This probe separates them. For each function it computes the *base* — the
//! lowest `::tempN` the function uses — and compares the two instruction streams
//! with each temporary renumbered relative to its own function's base. If the
//! streams then match, the function differs **only** by a uniform offset `k`, and
//! the code is right. Functions are then ordered by base, which recovers the
//! order the CK compiled them in, and the first function where `k` becomes
//! non-zero is where the drift started.
//!
//! ```text
//! cargo run --release --example drift -- Scripts.zip "Misc.bsa"
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
use std::collections::BTreeMap;
use std::io::Read;

/// The index of a `::tempN` operand, or `None` for anything else.
fn temp_index(name: &str) -> Option<i32> {
    let rest = name.strip_prefix("::temp")?;
    rest.parse().ok()
}

/// One instruction as a canonical string with every temporary shifted by `-base`.
fn canonical(
    p: &PexFile,
    mine: Option<&[String]>,
    f: &Function,
    base: i32,
    strip_offsets: bool,
) -> Vec<String> {
    let s = |i: u16| match mine {
        Some(t) => t.get(i as usize).cloned().unwrap_or_default(),
        None => p.text_or_empty(i).to_string(),
    };
    f.instructions
        .iter()
        .map(|i| {
            let mut out = opcodes::name(i.opcode).to_string();
            for (n, a) in i.args.iter().chain(i.varargs.iter()).enumerate() {
                // A jump's offset is relative to its own instruction, so it is
                // already position-independent once the streams are compared in
                // order — but a *target* difference is a real difference, so it
                // is kept unless explicitly stripped.
                let is_jump = matches!(opcodes::name(i.opcode), "jmp" | "jmpt" | "jmpf");
                if is_jump && n == 1 && strip_offsets {
                    out.push_str(" <off>");
                    continue;
                }
                out.push(' ');
                out.push_str(&match a {
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

/// The lowest `::tempN` index a function uses, ignoring `::NoneVar`.
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

    let mut total = 0usize;
    let mut aligned = 0usize; // structurally identical at some offset k
    let mut k_zero = 0usize; // ...and that offset is 0
    let mut offsets: BTreeMap<i32, usize> = BTreeMap::new();
    // The first function per script where k stops being 0.
    let mut first_drift: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut drifted_scripts = 0usize;

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

        // Recover compile order from the game's own numbering: the counter is
        // monotonic, so a function whose lowest temp is smaller was compiled
        // earlier.
        let mut order: Vec<(&Function, String)> = Vec::new();
        for gstate in g.objects.iter().flat_map(|o| o.states.iter()) {
            for gf in &gstate.functions {
                order.push((gf, g.text_or_empty(gstate.name).to_ascii_lowercase()));
            }
        }
        order.sort_by_key(|(f, _)| base_of(&g, None, f));

        let mut script_drifted = false;
        for (gf, want_state) in &order {
            let gname = g.text_or_empty(gf.name).to_string();
            let mine = c
                .states
                .iter()
                .find(|(s, _)| {
                    c.strings
                        .get(*s as usize)
                        .map(|n| n.to_ascii_lowercase() == *want_state)
                        .unwrap_or(false)
                })
                .and_then(|(_, fs)| {
                    fs.iter().find(|f| {
                        c.strings.get(f.name as usize).map(String::as_str) == Some(gname.as_str())
                    })
                });
            let Some(m) = mine else { continue };
            total += 1;

            let gb = base_of(&g, None, gf);
            let mb = base_of(&g, Some(&c.strings), m);
            let gt = canonical(&g, None, gf, gb, true);
            let mt = canonical(&g, Some(&c.strings), m, mb, true);
            if gt != mt {
                continue;
            }
            aligned += 1;
            let k = mb - gb;
            *offsets.entry(k).or_default() += 1;
            if k == 0 {
                k_zero += 1;
            } else if !script_drifted {
                script_drifted = true;
                let e = first_drift.entry(k.to_string()).or_default();
                if e.len() < 3 {
                    e.push(format!("{leaf}::{gname} base game {gb} mine {mb}"));
                }
            }
        }
        if script_drifted {
            drifted_scripts += 1;
        }
    }

    println!("functions paired: {total}");
    println!(
        "structurally identical at SOME offset: {aligned} ({:.1}%)",
        100.0 * aligned as f64 / total as f64
    );
    println!(
        "  ...of which offset 0 (byte-identical): {k_zero} ({:.1}%)",
        100.0 * k_zero as f64 / total as f64
    );
    println!();
    println!("offset histogram (mine - game base):");
    let mut v: Vec<_> = offsets.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    for (k, n) in v.iter().take(12) {
        println!("  k={k:>3}  {n}");
    }
    println!();
    println!("scripts where drift starts ({drifted_scripts}), by offset:");
    let mut v: Vec<_> = first_drift.iter().collect();
    v.sort_by_key(|r| std::cmp::Reverse(r.1.len()));
    for (k, e) in v.iter().take(10) {
        println!("  first offset {k}:");
        for x in e.iter() {
            println!("      {x}");
        }
    }
}
