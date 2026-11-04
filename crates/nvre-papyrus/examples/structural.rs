//! The first difference that is NOT explained by temporary numbering.
//!
//! `behaviour` buckets a function by its first differing instruction, but a
//! uniform offset in the temporary counter makes almost every operand differ, so
//! its buckets are dominated by "operand value on …" entries that are only
//! renumbering. `drift` removes that by renumbering both sides relative to their
//! own base, but only reports whether the whole function matches.
//!
//! This combines them: renumber both sides, then report the first instruction
//! that still differs, with both texts. What survives is a real structural
//! defect, and the bucket key is the mnemonic pair plus which operand position
//! disagrees.
//!
//! ```text
//! cargo run --release --example structural -- Scripts.zip "Misc.bsa"
//! ```

#[path = "support/mod.rs"]
mod support;

use nvre_papyrus::emit::Compiler;
use nvre_papyrus::pex::Function;
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

/// One instruction, with temporaries renumbered relative to `base` and a jump's
/// offset elided (a displaced target is a *symptom*, reported separately).
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
    let table = support::class_table(&mut zip, &sources);

    let mut total = 0usize;
    let mut same = 0usize;
    let mut buckets: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();

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
        // The class table from every source, so an inherited name has a type —
        // see `support::class_table`.
        let symbols = nvre_papyrus::emit::Symbols::for_script(&script, &table);
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
                total += 1;

                let gt = canon(&g, None, gf, base_of(&g, None, gf));
                let mt = canon(&g, Some(&c.strings), m, base_of(&g, Some(&c.strings), m));
                if gt == mt {
                    same += 1;
                    continue;
                }
                // First instruction that still differs.
                let pos = gt.iter().zip(&mt).position(|(x, y)| x != y);
                let Some(pos) = pos else {
                    // One side is a prefix of the other: a length difference.
                    let (short, long, who) = if gt.len() < mt.len() {
                        (gt.len(), mt.len(), "mine longer")
                    } else {
                        (mt.len(), gt.len(), "game longer")
                    };
                    let key = format!("{who}: {} vs {}", short, long);
                    let e = buckets.entry(key).or_insert((0, Vec::new()));
                    e.0 += 1;
                    if e.1.len() < 3 {
                        e.1.push(format!("{leaf}::{gname}"));
                    }
                    continue;
                };
                let gv = &gt[pos];
                let mv = &mt[pos];
                // Which operand position first disagrees, if the mnemonics match.
                let key = if gv[0] != mv[0] {
                    format!("opcode: mine {} vs game {}", mv[0], gv[0])
                } else {
                    let at = gv.iter().zip(mv).position(|(x, y)| x != y).unwrap_or(0);
                    format!("{} operand {at}", gv[0])
                };
                let e = buckets.entry(key).or_insert((0, Vec::new()));
                e.0 += 1;
                if e.1.len() < 3 {
                    e.1.push(format!(
                        "{leaf}::{gname} [{pos}] game `{}` mine `{}`",
                        gv.join(" "),
                        mv.join(" ")
                    ));
                }
            }
        }
    }

    println!("functions paired: {total}");
    println!(
        "structurally identical after renumbering: {same} ({:.1}%)",
        100.0 * same as f64 / total as f64
    );
    println!();
    let mut v: Vec<_> = buckets.iter().collect();
    v.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    for (k, (n, ex)) in v.iter().take(20) {
        println!("{n:6}  {k}");
        for e in ex {
            println!("            {e}");
        }
    }
}
