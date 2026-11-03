//! Dump one shipped `.pex` by name, so a disagreement can be looked at directly.
//!
//! ```text
//! cargo run --release --example dumpone -- "Skyrim - Misc.bsa" bladetrapit
//! ```

use nvre_papyrus::{opcodes, PexFile, Value};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: dumpone <archive> <script-name>");
        return;
    }
    let mut archive = bsa105::Archive::open(&a[1]).unwrap();
    let path = format!("scripts\\{}.pex", a[2].to_ascii_lowercase());
    let bytes = archive.extract_by_path(&path).unwrap();
    let f = PexFile::parse(&bytes).unwrap();

    let s = |i: nvre_papyrus::pex::StrIndex| f.text_or_empty(i).to_string();
    for o in &f.objects {
        println!("object {}", s(o.name));
        for st in &o.states {
            println!("  state {:?}", s(st.name));
            for fnx in &st.functions {
                println!(
                    "    body {} : {}  locals {:?}",
                    s(fnx.name),
                    s(fnx.return_type),
                    fnx.locals
                        .iter()
                        .map(|t| format!("{}:{}", s(t.name), s(t.type_name)))
                        .collect::<Vec<_>>()
                );
                for (n, i) in fnx.instructions.iter().enumerate() {
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
                        "      {n:3} {:<12} {}",
                        opcodes::name(i.opcode),
                        args.join(" ")
                    );
                }
            }
        }
    }
}
