//! Which source construct makes the parser record a local whose **type** is
//! `end`?
//!
//! `typecensus` reports 336 functions where we declare a local typed `end` and
//! the game declares none. `end` is not a Papyrus type, so this is the parser
//! reading a keyword as a type name. This prints the offending source line.
//!
//! ```text
//! cargo run --release --example endlocals -- Scripts.zip
//! ```

use std::collections::BTreeMap;
use std::io::Read;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&a[1]).unwrap()).unwrap();

    let mut names = Vec::new();
    for i in 0..zip.len() {
        let n = zip.by_index(i).unwrap().name().to_string();
        if n.ends_with(".psc") {
            names.push(n);
        }
    }
    names.sort();

    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut shown = 0;
    let mut bad_scripts = 0;

    for name in &names {
        let src = {
            let mut f = zip.by_name(name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            nvre_util::cp1252::decode(&b)
        };
        let Ok(script) = nvre_papyrus::parse(&nvre_papyrus::lex(&src)) else {
            continue;
        };
        let lines: Vec<&str> = src.lines().collect();
        let mut hit = false;
        for f in script.decls.iter().filter_map(|d| match &d.kind {
            nvre_papyrus::parse::DeclKind::Function(f) => Some(f),
            _ => None,
        }) {
            for l in &f.locals {
                let t = l.type_name.0.to_ascii_lowercase();
                if t == "end" || t == "endif" || t == "endfunction" || t == "endwhile" {
                    hit = true;
                    *by_type.entry(t.clone()).or_default() += 1;
                    if shown < 10 {
                        shown += 1;
                        let line = lines
                            .get(l.line as usize - 1)
                            .copied()
                            .unwrap_or("<no line>");
                        println!(
                            "{} :: {} local `{}` typed `{}` at line {}: {}",
                            name.rsplit('/').next().unwrap(),
                            f.name,
                            l.name,
                            l.type_name.0,
                            l.line,
                            line.trim()
                        );
                    }
                }
            }
        }
        if hit {
            bad_scripts += 1;
        }
    }
    println!();
    println!("scripts affected: {bad_scripts}");
    for (k, v) in &by_type {
        println!("  type `{k}`: {v} locals");
    }
}
