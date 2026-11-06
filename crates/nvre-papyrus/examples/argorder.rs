//! Probe: does a call evaluate its arguments left to right, or right to left?
//!
//! ```text
//! cargo run --release --example argorder -- "Skyrim - Misc.bsa"
//! ```
//!
//! The emitter evaluates left to right, and that was adopted on a **two
//! function** margin over right to left — which is not a measurement. This
//! settles it from the shipped bytes alone, without compiling anything.
//!
//! The method: for every call instruction with at least two arguments that are
//! each written by an earlier instruction in the same function, compare the
//! indices of the instructions that defined them.
//!
//! ```text
//! RemoveItem(Gold, AnimalGold.value as int)      AnimalTrainerSystemScript
//!   1 propget value ::AnimalGold_var ::temp1   <- defines arg 1
//!   2 cast    ::temp2 ::temp1                  <- defines arg 1
//!   3 cast    ::temp3 ::Gold_var               <- defines arg 0
//!   4 callmethod RemoveItem ::temp0 ::NoneVar ::temp3 ::temp2 ...
//! ```
//!
//! Argument 1 is defined at 2 and argument 0 at 3, so the arguments were
//! evaluated **right to left** there. A left-to-right compiler would have
//! written argument 0's cast first. Counting that comparison over the whole
//! corpus says which order the compiler actually used, and how often the other
//! order appears — a margin of a handful is noise, so the ratio is the answer.
use nvre_papyrus::opcodes;
use nvre_papyrus::{PexFile, Value};

/// Which operand receives the result, from the measured table.
fn dest_index(opcode: u8) -> Option<usize> {
    let name = opcodes::name(opcode);
    match name {
        "callmethod" | "callparent" | "callstatic" | "propget" => Some(2),
        "assign" | "cast" | "strcat" | "not" | "ineg" | "fneg" | "array_getelement" => Some(0),
        n if n.starts_with("cmp_") => Some(0),
        n if n.starts_with("i") || n.starts_with("f") => {
            // iadd/isub/imul/idiv/imod and their float twins: three operands,
            // destination first. `if_*` and `fmod` are covered by the prefix
            // test and are not instructions in Skyrim's table.
            Some(0)
        }
        _ => None,
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 2 {
        eprintln!("usage: argorder <archive.bsa>");
        return;
    }
    let mut archive = bsa105::Archive::open(&a[1]).unwrap();
    let paths: Vec<String> = archive
        .entries()
        .map(|e| e.path())
        .filter(|p| p.to_ascii_lowercase().ends_with(".pex"))
        .collect();

    let mut ltr = 0usize;
    let mut rtl = 0usize;
    let mut equal = 0usize;
    // The decisive subset: calls whose arguments are **all** plain names that
    // need nothing but a slot-conversion `cast`. No nesting, no argument that
    // computes anything, so the order the casts appear in is the order the
    // compiler wrote the argument list — the whole question, with nothing to
    // confound it.
    let mut pure_ltr = 0usize;
    let mut pure_rtl = 0usize;
    let mut pure_examples: Vec<String> = Vec::new();
    let mut functions_scanned = 0usize;
    let mut rtl_examples: Vec<String> = Vec::new();
    let mut ltr_examples: Vec<String> = Vec::new();

    for path in &paths {
        let Ok(bytes) = archive.extract_by_path(path) else {
            continue;
        };
        let Ok(f) = PexFile::parse(&bytes) else {
            continue;
        };
        for obj in &f.objects {
            for st in &obj.states {
                for func in &st.functions {
                    functions_scanned += 1;
                    let ins = &func.instructions;
                    for i in 0..ins.len() {
                        let info = opcodes::info(ins[i].opcode);
                        let Some(info) = info else { continue };
                        if !info.varargs || ins[i].varargs.len() < 2 {
                            continue;
                        }
                        // The definition index of an argument: the latest
                        // instruction before `i` that writes that name from its
                        // destination position.
                        let def_of = |want: &Value| -> Option<usize> {
                            let Value::Identifier(want) = want else {
                                return None;
                            };
                            let mut found = None;
                            for (j, prev) in ins.iter().enumerate().take(i) {
                                let Some(d) = dest_index(prev.opcode) else {
                                    continue;
                                };
                                let mut all = prev.args.iter().chain(prev.varargs.iter());
                                if all
                                    .nth(d)
                                    .is_some_and(|v| matches!(v, Value::Identifier(x) if x == want))
                                {
                                    found = Some(j);
                                }
                            }
                            found
                        };
                        // The pure case: every argument is a plain name whose
                        // only instruction is a slot-conversion `cast`. Then
                        // the order the casts appear in *is* the argument
                        // order, and nothing else can explain it.
                        let mut defs: Vec<(usize, bool)> = Vec::new();
                        for v in &ins[i].varargs {
                            let Some(d) = def_of(v) else {
                                defs.clear();
                                break;
                            };
                            let is_cast = opcodes::name(ins[d].opcode) == "cast";
                            let from_plain = ins[d]
                                .args
                                .get(1)
                                .map(|s| match s {
                                    Value::Identifier(x) => {
                                        !f.text_or_empty(*x).starts_with("::temp")
                                    }
                                    Value::None => true,
                                    _ => false,
                                })
                                .unwrap_or(false);
                            defs.push((d, is_cast && from_plain));
                        }
                        if defs.len() >= 2 && defs.iter().all(|(_, pure)| *pure) {
                            let ascending = defs.windows(2).all(|w| w[0].0 < w[1].0);
                            let descending = defs.windows(2).all(|w| w[0].0 > w[1].0);
                            let name = format!(
                                "{}::{} [{}]",
                                f.text_or_empty(obj.name),
                                f.text_or_empty(func.name),
                                i
                            );
                            if ascending && !descending {
                                pure_ltr += 1;
                                if pure_examples.len() < 4 {
                                    pure_examples.push(format!("{name} LTR"));
                                }
                            } else if descending {
                                pure_rtl += 1;
                                if pure_examples.len() < 4 {
                                    pure_examples.push(format!("{name} RTL"));
                                }
                            }
                        }
                        for k in 0..ins[i].varargs.len() - 1 {
                            let (Some(d0), Some(d1)) =
                                (def_of(&ins[i].varargs[k]), def_of(&ins[i].varargs[k + 1]))
                            else {
                                continue;
                            };
                            if d0 < d1 {
                                ltr += 1;
                                if ltr_examples.len() < 5 {
                                    ltr_examples.push(format!(
                                        "{}::{} [{}] {} args {} then {}",
                                        f.text_or_empty(obj.name),
                                        f.text_or_empty(func.name),
                                        i,
                                        opcodes::name(ins[i].opcode),
                                        d0,
                                        d1
                                    ));
                                }
                            } else if d0 > d1 {
                                rtl += 1;
                                if rtl_examples.len() < 5 {
                                    rtl_examples.push(format!(
                                        "{}::{} [{}] {} args {} then {}",
                                        f.text_or_empty(obj.name),
                                        f.text_or_empty(func.name),
                                        i,
                                        opcodes::name(ins[i].opcode),
                                        d0,
                                        d1
                                    ));
                                }
                            } else {
                                equal += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    let total = ltr + rtl + equal;
    println!("files {}", paths.len());
    println!("functions {functions_scanned}");
    println!("argument pairs whose order the bytes decide: {total}");
    println!(
        "  left to right  {ltr}  ({:.1}%)",
        100.0 * ltr as f64 / total as f64
    );
    println!(
        "  right to left  {rtl}  ({:.1}%)",
        100.0 * rtl as f64 / total as f64
    );
    println!("  same slot      {equal}");
    println!();
    println!("calls whose every argument is a bare name needing only a `cast`:");
    println!("  left to right                  {pure_ltr}");
    println!("  right to left                  {pure_rtl}");
    for e in &pure_examples {
        println!("    {e}");
    }
    println!("\nleft-to-right witnesses:");
    for e in &ltr_examples {
        println!("  {e}");
    }
    println!("\nright-to-left witnesses:");
    for e in &rtl_examples {
        println!("  {e}");
    }
}
