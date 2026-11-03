//! Print exactly what the compiler produced, state by state.
//!
//! ```text
//! cargo run --release --example cmpstates -- some.psc
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = nvre_util::cp1252::decode(&std::fs::read(&a[1]).unwrap());
    let s = parse(&lex(&src)).unwrap();
    let sy = Symbols {
        script: s.name.clone(),
        parent: s.parent.clone().unwrap_or_default(),
        variables: Vec::new(),
        properties: Vec::new(),
        states: Vec::new(),
    };
    let c = Compiler::new(&s, sy).compile().unwrap();
    println!("states emitted: {}", c.states.len());
    for (n, fs) in &c.states {
        let name = c.strings.get(*n as usize).cloned().unwrap_or_default();
        let names: Vec<String> = fs
            .iter()
            .map(|f| c.strings.get(f.name as usize).cloned().unwrap_or_default())
            .collect();
        println!("  state {name:?}: {names:?}");
    }
}
