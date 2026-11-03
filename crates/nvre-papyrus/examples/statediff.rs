use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, PexFile};
use std::collections::BTreeSet;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = nvre_util::cp1252::decode(&std::fs::read(&a[1]).unwrap());
    let s = parse(&lex(&src)).unwrap();
    let symbols = Symbols {
        script: s.name.clone(),
        parent: s.parent.clone().unwrap_or_default(),
        variables: Vec::new(),
        properties: Vec::new(),
        states: Vec::new(),
    };
    let c = Compiler::new(&s, symbols).compile().unwrap();
    let g = PexFile::parse(&std::fs::read(&a[2]).unwrap()).unwrap();
    for (sn, funcs) in &c.states {
        let name = c.strings.get(*sn as usize).cloned().unwrap_or_default();
        let mine: BTreeSet<String> = funcs
            .iter()
            .map(|f| {
                c.strings
                    .get(f.name as usize)
                    .cloned()
                    .unwrap_or_default()
                    .to_lowercase()
            })
            .collect();
        let theirs: BTreeSet<String> = g
            .objects
            .first()
            .map(|o| {
                o.states
                    .iter()
                    .find(|st| g.text_or_empty(st.name).eq_ignore_ascii_case(&name))
                    .map(|st| {
                        st.functions
                            .iter()
                            .map(|f| g.text_or_empty(f.name).to_lowercase())
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        println!("state {name:?}");
        println!(
            "  only mine: {:?}",
            mine.difference(&theirs).collect::<Vec<_>>()
        );
        println!(
            "  only game: {:?}",
            theirs.difference(&mine).collect::<Vec<_>>()
        );
    }
}
