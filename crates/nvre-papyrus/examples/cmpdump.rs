//! Print the parse tree's declarations: which state each function is in.
//!
//! ```text
//! cargo run --release --example cmpdump -- some.psc
//! ```

use nvre_papyrus::parse::DeclKind;
use nvre_papyrus::{lex, parse};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = nvre_util::cp1252::decode(&std::fs::read(&a[1]).unwrap());
    let s = parse(&lex(&src)).unwrap();
    println!("decls: {}", s.decls.len());
    for d in &s.decls {
        match &d.kind {
            DeclKind::State(st) => {
                println!(
                    "  STATE {:?} with {} functions",
                    st.name,
                    st.functions.len()
                );
                for f in &st.functions {
                    println!("     fn {}", f.name);
                }
            }
            DeclKind::Function(f) => println!("  script-level fn {}", f.name),
            DeclKind::Variable(v) => println!("  var {}", v.name),
            DeclKind::Property(p) => println!("  prop {}", p.name),
            DeclKind::Struct(st) => println!("  struct {}", st.name),
        }
    }
}
