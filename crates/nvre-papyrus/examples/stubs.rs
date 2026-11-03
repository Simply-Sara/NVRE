//! Detail on how the compiler represents members a script inherits but does not
//! declare.
//!
//! `activemagiceffect.psc` declares nothing at all, yet its `.pex` lists 92
//! functions. Those are the compiler writing out the parent's members. This
//! prints one of them in full — flags, return type, parameters, doc, locals,
//! instructions, and its debug-table line numbers — so the shape can be
//! reproduced rather than guessed.
//!
//! ```text
//! cargo run --release --example stubs -- activemagiceffect.pex
//! ```

use nvre_papyrus::{opcodes, PexFile, Value};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let f = PexFile::parse(&bytes).unwrap();
    let s = |i: nvre_papyrus::pex::StrIndex| f.text_or_empty(i).to_string();

    // The debug block, so a stub's line numbers are visible.
    let mut dbg: std::collections::BTreeMap<(String, String), Vec<u16>> =
        std::collections::BTreeMap::new();
    for d in &f.debug.functions {
        dbg.insert((s(d.state), s(d.function)), d.lines.clone());
    }

    let obj = &f.objects[0];
    println!("object {}  variables {}", s(obj.name), obj.variables.len());
    println!("user flags: {}", f.user_flags.len());

    for st in &obj.states {
        for fun in &st.functions {
            let name = s(fun.name);
            let params: Vec<String> = fun
                .params
                .iter()
                .map(|p| format!("{} {}", s(p.type_name), s(p.name)))
                .collect();
            let locals: Vec<String> = fun
                .locals
                .iter()
                .map(|l| format!("{}:{}", s(l.name), s(l.type_name)))
                .collect();
            let lines = dbg
                .get(&(s(st.name), name.clone()))
                .cloned()
                .unwrap_or_default();
            println!(
                "{:<28} ret={:<18} flags={:<3} params({}) doc={:?} locals={:?} lines={:?} instrs={}",
                name,
                s(fun.return_type),
                fun.flags,
                params.join(", "),
                s(fun.doc),
                locals,
                lines,
                fun.instructions.len()
            );
            for i in &fun.instructions {
                let args: Vec<String> = i
                    .args
                    .iter()
                    .chain(i.varargs.iter())
                    .map(|v| match v {
                        Value::None => "None".to_string(),
                        Value::Identifier(x) => s(*x),
                        Value::String(x) => format!("{:?}", s(*x)),
                        Value::Int(x) => x.to_string(),
                        Value::Float(x) => x.to_string(),
                        Value::Bool(x) => x.to_string(),
                    })
                    .collect();
                println!("      {} {}", opcodes::name(i.opcode), args.join(" "));
            }
        }
    }
}
