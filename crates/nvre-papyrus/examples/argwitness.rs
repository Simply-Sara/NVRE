//! Probe: full instruction windows around calls whose arguments are distinct
//! bare names, so the evaluation order is readable rather than counted.
//!
//! ```text
//! cargo run --release --example argwitness -- "Skyrim - Misc.bsa" [limit]
//! ```
//!
//! `argorder` counts the two answers; this shows the bytes behind the count.
//! It prints only calls where **every** argument is a plain name written by an
//! earlier instruction, so nothing but the argument list explains the order.
use nvre_papyrus::opcodes;
use nvre_papyrus::{PexFile, Value};

fn dest_index(opcode: u8) -> Option<usize> {
    let name = opcodes::name(opcode);
    match name {
        "callmethod" | "callparent" | "callstatic" | "propget" => Some(2),
        n if n.starts_with("cmp_") => Some(0),
        "assign" | "cast" | "strcat" | "not" | "ineg" | "fneg" | "array_getelement" => Some(0),
        _ => None,
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 2 {
        eprintln!("usage: argwitness <archive.bsa> [limit]");
        return;
    }
    let limit: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(12);
    let mut archive = bsa105::Archive::open(&a[1]).unwrap();
    let paths: Vec<String> = archive
        .entries()
        .map(|e| e.path())
        .filter(|p| p.to_ascii_lowercase().ends_with(".pex"))
        .collect();

    let mut shown = 0usize;
    'outer: for path in &paths {
        let Ok(bytes) = archive.extract_by_path(path) else {
            continue;
        };
        let Ok(f) = PexFile::parse(&bytes) else {
            continue;
        };
        for obj in &f.objects {
            for st in &obj.states {
                for func in &st.functions {
                    let ins = &func.instructions;
                    for i in 0..ins.len() {
                        let Some(info) = opcodes::info(ins[i].opcode) else {
                            continue;
                        };
                        if !info.varargs || ins[i].varargs.len() < 2 {
                            continue;
                        }
                        // Each argument must be a temporary written by an
                        // earlier `cast` whose source is a plain (non-temporary)
                        // name — so the call's arguments are the *outputs* of
                        // those casts, and their order in the stream is the order
                        // the compiler evaluated the argument list.
                        let mut shown_args: Vec<String> = Vec::new();
                        let mut sources: Vec<String> = Vec::new();
                        let mut defs: Vec<usize> = Vec::new();
                        let mut ok = true;
                        for v in &ins[i].varargs {
                            let Value::Identifier(x) = v else {
                                ok = false;
                                break;
                            };
                            let mut found = None;
                            for (j, prev) in ins.iter().enumerate().take(i) {
                                if opcodes::name(prev.opcode) != "cast" {
                                    continue;
                                }
                                let from_plain = prev.args.get(1).is_some_and(|s| match s {
                                    Value::Identifier(y) => {
                                        !f.text_or_empty(*y).starts_with("::temp")
                                    }
                                    Value::None => true,
                                    _ => false,
                                });
                                let writes = prev
                                    .args
                                    .first()
                                    .is_some_and(|d| matches!(d, Value::Identifier(w) if w == x));
                                if writes && from_plain {
                                    found = Some(j);
                                }
                            }
                            match found {
                                Some(j) => {
                                    defs.push(j);
                                    if let Some(Value::Identifier(src)) = ins[j].args.get(1) {
                                        sources.push(f.text_or_empty(*src).to_string());
                                    } else {
                                        sources.push("None".to_string());
                                    }
                                }
                                None => {
                                    ok = false;
                                    break;
                                }
                            }
                            shown_args.push(f.text_or_empty(*x).to_string());
                        }
                        if !ok || defs.len() < 2 {
                            continue;
                        }
                        // Distinct sources, so the order is not ambiguous the
                        // way `cast(self, self)` is.
                        let mut sorted = sources.clone();
                        sorted.sort();
                        sorted.dedup();
                        if sorted.len() != sources.len() {
                            continue;
                        }
                        println!(
                            "==== {}::{} [{}] {}   casts at {:?}",
                            f.text_or_empty(obj.name),
                            f.text_or_empty(func.name),
                            i,
                            opcodes::name(ins[i].opcode),
                            defs
                        );
                        let lo = i.saturating_sub(10);
                        for (j, one) in ins.iter().enumerate().take(i + 1).skip(lo) {
                            let ops: Vec<String> = one
                                .args
                                .iter()
                                .chain(one.varargs.iter())
                                .map(|v| match v {
                                    Value::Identifier(x) => f.text_or_empty(*x).to_string(),
                                    Value::String(x) => {
                                        format!("{:?}", f.text_or_empty(*x))
                                    }
                                    Value::Int(n) => n.to_string(),
                                    Value::Float(n) => n.to_string(),
                                    Value::Bool(b) => b.to_string(),
                                    Value::None => "None".to_string(),
                                })
                                .collect();
                            println!(
                                "  {j:4} {:<12} {}",
                                opcodes::name(one.opcode),
                                ops.join(" ")
                            );
                        }
                        println!("       args in order: {}", shown_args.join(", "));
                        println!();
                        shown += 1;
                        if shown >= limit {
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
}
