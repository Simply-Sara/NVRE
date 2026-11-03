//! Dump a plain `.pex` file — no archive needed.
//!
//! ```text
//! cargo run --release --example dumppex -- /path/to/x.pex
//! ```

use nvre_papyrus::{opcodes, PexFile, Value};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let Some(path) = a.get(1) else {
        eprintln!("usage: dumppex <file.pex>");
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let f = PexFile::parse(&bytes).unwrap();
    let s = |i: nvre_papyrus::pex::StrIndex| f.text_or_empty(i).to_string();

    for o in &f.objects {
        for st in &o.states {
            println!("state {:?}", s(st.name));
            for fun in &st.functions {
                let locals: Vec<String> = fun
                    .locals
                    .iter()
                    .map(|t| format!("{}:{}", s(t.name), s(t.type_name)))
                    .collect();
                println!("  fn {} () locals [{}]", s(fun.name), locals.join(", "));
                for (n, i) in fun.instructions.iter().enumerate() {
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
                    println!(
                        "    {n:3} {:<14} {}",
                        opcodes::name(i.opcode),
                        args.join(" ")
                    );
                }
            }
        }
    }
}
