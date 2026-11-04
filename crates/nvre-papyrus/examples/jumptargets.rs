//! Why do jump targets disagree?
//!
//! `behaviour` reports ~730 `jmpf` and ~550 `jmp` operand differences. Those are
//! the only remaining category where a difference changes control flow, so they
//! matter more than their count suggests.
//!
//! Two very different causes look identical in a diff:
//!
//!   * a **length** difference upstream — one missing instruction earlier in the
//!     function shifts every target after it, and the target arithmetic itself is
//!     fine;
//!   * a **computation** difference — both functions are the same length and the
//!     same shape, but the offset is still wrong, which would mean the jump
//!     convention is wrong somewhere.
//!
//! This separates them, and for the length cases reports where the two functions
//! first diverge in *length* rather than in content.
//!
//! ```text
//! cargo run --release --example jumptargets -- Scripts.zip "Misc.bsa"
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
use std::collections::BTreeMap;
use std::io::Read;

fn text(p: &PexFile, mine: Option<&[String]>, f: &Function) -> Vec<String> {
    let s = |i: u16| match mine {
        Some(t) => t.get(i as usize).cloned().unwrap_or_default(),
        None => p.text_or_empty(i).to_string(),
    };
    f.instructions
        .iter()
        .map(|i| {
            let mut out = opcodes::name(i.opcode).to_string();
            for a in i.args.iter().chain(i.varargs.iter()) {
                out.push(' ');
                out.push_str(&match a {
                    Value::None => "None".to_string(),
                    Value::Identifier(x) => s(*x),
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

/// Strip the trailing offset from a jump so the *shape* can be compared.
fn without_offset(line: &str) -> String {
    let mut parts: Vec<&str> = line.split(' ').collect();
    let is_jump = matches!(
        parts.first().copied(),
        Some("jmp") | Some("jmpt") | Some("jmpf")
    );
    if is_jump && parts.len() > 1 {
        parts.pop();
    }
    parts.join(" ")
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

    let mut same_len = 0usize;
    let mut diff_len = 0usize;
    let mut shape_differs = 0usize;
    let mut examples: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    // For length cases: which opcode first appears in one side and not the other?
    let mut first_extra: BTreeMap<String, usize> = BTreeMap::new();

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
                            c.strings.get(f.name as usize).map(String::as_str)
                                == Some(gname.as_str())
                        })
                    });
                let Some(m) = mine else { continue };
                let gt = text(&g, None, gf);
                let mt = text(&g, Some(&c.strings), m);
                if gt == mt {
                    continue;
                }
                // Only functions whose FIRST difference is a jump target.
                let Some(pos) = gt.iter().zip(&mt).position(|(x, y)| x != y) else {
                    continue;
                };
                let is_jump = matches!(
                    gt[pos].split(' ').next(),
                    Some("jmp") | Some("jmpt") | Some("jmpf")
                );
                if !is_jump {
                    continue;
                }
                if without_offset(&gt[pos]) != without_offset(&mt[pos]) {
                    shape_differs += 1;
                    continue;
                }
                if gt.len() == mt.len() {
                    same_len += 1;
                    let e = examples.entry("same length").or_default();
                    if e.len() < 8 {
                        e.push(format!(
                            "{leaf}::{gname} [{pos}] game {} / mine {}",
                            gt[pos], mt[pos]
                        ));
                    }
                } else {
                    diff_len += 1;
                    // Where do the lengths first diverge?
                    let d = gt.iter().zip(&mt).position(|(x, y)| x != y);
                    let e = examples.entry("different length").or_default();
                    if e.len() < 8 {
                        e.push(format!(
                            "{leaf}::{gname} len {} vs {} at {:?}: game {} / mine {}",
                            gt.len(),
                            mt.len(),
                            d,
                            gt.get(pos).map(String::as_str).unwrap_or(""),
                            mt.get(pos).map(String::as_str).unwrap_or("")
                        ));
                    }
                    // Which instruction does one side have that the other lacks,
                    // near the divergence?
                    if let Some(d) = d {
                        for off in 0..6 {
                            let gi = gt.get(d + off).map(|l| l.split(' ').next().unwrap_or(""));
                            let mi = mt.get(d + off).map(|l| l.split(' ').next().unwrap_or(""));
                            if gi.is_some() && gi != mi {
                                *first_extra
                                    .entry(format!(
                                        "{} vs {}",
                                        mi.unwrap_or("-"),
                                        gi.unwrap_or("-")
                                    ))
                                    .or_default() += 1;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    println!("functions whose FIRST difference is a jump offset:");
    println!("  same instruction count  (target arithmetic): {same_len}");
    println!("  different count         (a missing instruction upstream): {diff_len}");
    println!("  jump shape differs      (not a target problem): {shape_differs}");
    println!();
    for (k, v) in &examples {
        println!("{k}:");
        for e in v {
            println!("   {e}");
        }
        println!();
    }
    println!("for the length cases, the first opcode mismatch after the divergence:");
    let mut v: Vec<_> = first_extra.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, n) in v.iter().take(12) {
        println!(
            "  {n:5}  mine {} / game {}",
            k.split(" vs ").next().unwrap_or(""),
            k.split(" vs ").nth(1).unwrap_or("")
        );
    }
}
