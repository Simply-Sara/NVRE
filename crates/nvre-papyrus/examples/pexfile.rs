//! Dump a **loose** `.pex` file: every function's locals and instructions.
//!
//! ```text
//! cargo run --release --example pexfile -- path/to/P.pex
//! ```
//!
//! `dumpone` reads from a BSA and `dumppex` takes an archive path; this one takes
//! a plain file, which is what the Creation Kit's oracle produces
//! (`scripts/ck-compile.sh`). It is the tool for reading a controlled experiment:
//! one probe source in, one `.pex` out, and the compiler's own naming and
//! numbering visible without a corpus run.
use nvre_papyrus::{opcodes, PexFile, Value};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 2 {
        eprintln!("usage: pexfile <file.pex>");
        return;
    }
    let bytes = std::fs::read(&a[1]).expect("read the file");
    let f = PexFile::parse(&bytes).expect("parse the .pex");
    let s = |i: nvre_papyrus::pex::StrIndex| f.text_or_empty(i).to_string();

    for o in &f.objects {
        println!("object {}", s(o.name));
        for st in &o.states {
            println!("  state {:?}", s(st.name));
            for func in &st.functions {
                println!(
                    "    body {} : {}  locals {:?}",
                    s(func.name),
                    s(func.return_type),
                    func.locals
                        .iter()
                        .map(|l| format!("{}:{}", s(l.name), s(l.type_name)))
                        .collect::<Vec<_>>()
                );
                for (i, ins) in func.instructions.iter().enumerate() {
                    let args: Vec<String> = ins
                        .args
                        .iter()
                        .chain(ins.varargs.iter())
                        .map(|v| match v {
                            Value::Identifier(x) => s(*x),
                            Value::String(x) => format!("{:?}", s(*x)),
                            Value::Int(n) => n.to_string(),
                            Value::Float(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            Value::None => "None".to_string(),
                        })
                        .collect();
                    println!("      {i:4} {:<14} {}", opcodes::name(ins.opcode), args.join(" "));
                }
            }
        }
    }
}
