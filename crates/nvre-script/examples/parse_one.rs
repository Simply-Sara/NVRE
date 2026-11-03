//! Probe: what the parser and emitter make of one line.
//!
//! ```text
//! cargo run -p nvre-script --example parse_one -- 'user.setAV variable08 -1'
//! ```
use nvre_script::parse::parse;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("pass one or more source lines");
        return;
    }
    for line in args {
        let source = format!("scn S\nBegin GameMode\n{line}\nEnd\n");
        let script = parse(&source);
        println!("{line}");
        for stmt in &script.stmts {
            match stmt {
                nvre_script::Stmt::Call { on, command, args } => {
                    let names: Vec<&str> = args
                        .iter()
                        .filter(|t| t.kind != nvre_script::lex::Kind::Space)
                        .map(|t| t.text.as_str())
                        .collect();
                    println!("  call on={on:?} {command} args({}) {names:?}", names.len());
                }
                other => println!("  {other:?}"),
            }
        }
        match nvre_script::compile(&source) {
            Ok(b) => println!("  bytes {b:02X?}"),
            Err(e) => println!("  error {e}"),
        }
    }
}
