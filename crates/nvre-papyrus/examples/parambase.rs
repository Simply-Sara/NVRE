//! What does an **unread parameter** cost the temporary counter?
//!
//! A function's first temporary is not always `::temp0`, and the difference
//! tracks its parameters. The emitter leaves this unmodelled, because two
//! candidate rules both fit the hand-built probes and both cost corpus-wide.
//!
//! This measures the relation from the shipped bytes: for every function, print
//! `(parameter count, parameters the body never reads) -> the game's base`, where
//! base is the lowest `::tempN` in the game's own locals table. A real rule has
//! one base per pair; a spread means there is no such rule.
//!
//! ```text
//! cargo run --release --example parambase -- Scripts.zip "Misc.bsa" [rows]
//! ```
//!
//! The parameter list comes from the **source** (a `.pex` has none), and "read"
//! is decided from the **game's** instruction stream: a parameter whose name
//! appears as an operand anywhere in the body is read.

#[path = "support/mod.rs"]
mod support;

use nvre_papyrus::parse::{DeclKind, Script};
use nvre_papyrus::{lex, parse, PexFile, Value};
use std::collections::BTreeMap;

/// `(name lowercase, unread)` for each parameter of the function called `name`.
fn params_of(script: &Script, want: &str) -> Option<Vec<(String, bool)>> {
    fn walk(kind: &DeclKind, want: &str) -> Option<Vec<(String, bool)>> {
        match kind {
            DeclKind::Function(f) if f.name.eq_ignore_ascii_case(want) => Some(
                f.params
                    .iter()
                    .map(|p| (p.name.to_ascii_lowercase(), false))
                    .collect(),
            ),
            DeclKind::State(s) => s
                .functions
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(want))
                .map(|f| {
                    f.params
                        .iter()
                        .map(|p| (p.name.to_ascii_lowercase(), false))
                        .collect()
                }),
            _ => None,
        }
    }
    script.decls.iter().find_map(|d| walk(&d.kind, want))
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let rows: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&a[1]).unwrap()).unwrap();
    let mut archive = bsa105::Archive::open(&a[2]).unwrap();

    let mut sources = Vec::new();
    for i in 0..zip.len() {
        let name = zip.by_index(i).unwrap().name().to_string();
        if name.ends_with(".psc") {
            sources.push(name);
        }
    }
    sources.sort();

    // (params, unread) -> base -> functions
    let mut table: BTreeMap<(usize, usize), BTreeMap<i32, usize>> = BTreeMap::new();
    let mut seen = 0usize;

    for name in &sources {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let Some(src) = support::source_of(&mut zip, name) else {
            continue;
        };
        let Ok(script) = parse(&lex(&src)) else {
            continue;
        };
        let Ok(g) = archive
            .extract_by_path(&format!("scripts\\{leaf}.pex"))
            .map_err(|e| e.to_string())
            .and_then(|b| PexFile::parse(&b).map_err(|e| e.to_string()))
        else {
            continue;
        };
        for obj in &g.objects {
            for st in &obj.states {
                for f in &st.functions {
                    let fname = g.text_or_empty(f.name).to_string();
                    if fname.eq_ignore_ascii_case("GetState")
                        || fname.eq_ignore_ascii_case("GotoState")
                    {
                        continue;
                    }
                    let Some(mut params) = params_of(&script, &fname) else {
                        continue;
                    };
                    if params.is_empty() {
                        continue;
                    }
                    // The base is the lowest `::tempN` the game declares, not the
                    // lowest it uses: a temporary that is allocated and never
                    // read still occupies a number and still appears in the
                    // locals table.
                    let mut base = i32::MAX;
                    for l in &f.locals {
                        if let Some(rest) = g.text_or_empty(l.name).strip_prefix("::temp") {
                            if let Ok(v) = rest.parse::<i32>() {
                                base = base.min(v);
                            }
                        }
                    }
                    if base == i32::MAX {
                        // No temporaries at all: a zero-cost body, no evidence.
                        continue;
                    }
                    let mut used: Vec<String> = Vec::new();
                    for i in &f.instructions {
                        for v in i.args.iter().chain(i.varargs.iter()) {
                            if let Value::Identifier(x) = v {
                                used.push(g.text_or_empty(*x).to_ascii_lowercase());
                            }
                        }
                    }
                    for p in params.iter_mut() {
                        p.1 = !used.iter().any(|u| *u == p.0);
                    }
                    let unread = params.iter().filter(|(_, u)| *u).count();
                    *table
                        .entry((params.len(), unread))
                        .or_default()
                        .entry(base)
                        .or_default() += 1;
                    seen += 1;
                }
            }
        }
    }

    println!("functions with parameters and temporaries: {seen}\n");
    println!("{:>7} {:>7}  {:>6}  base distribution", "params", "unread", "n");
    let mut keys: Vec<(&(usize, usize), &BTreeMap<i32, usize>)> = table.iter().collect();
    keys.sort_by_key(|(k, _)| **k);
    for ((params, unread), bases) in keys.iter().take(rows) {
        let n: usize = bases.values().sum();
        let shown: Vec<String> = bases
            .iter()
            .take(6)
            .map(|(b, c)| format!("{b}x{c}"))
            .collect();
        println!("{params:7} {unread:7}  {n:6}  {}", shown.join("  "));
    }
}
