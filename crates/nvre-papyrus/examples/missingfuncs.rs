//! Which functions does the shipped file have that the emitter produces none of?
//!
//! `failuremodes` counts 177 of these. A missing function is not a byte
//! difference — the compiled script is broken — so this is the highest-severity
//! remaining item even though it is the smallest bucket.
//!
//! This prints them by script so the *shape* is visible: are they all one
//! declaration form the parser accepts and the emitter drops, or are they
//! spread across several?
//!
//! ```text
//! cargo run --release --example missingfuncs -- Scripts.zip "Misc.bsa"
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, PexFile};
use std::collections::BTreeMap;
use std::io::Read;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let limit: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(60);

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

    // name -> how many scripts are missing it
    let mut by_name: BTreeMap<String, usize> = BTreeMap::new();
    let mut case_mismatch: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows: Vec<String> = Vec::new();
    let mut total = 0usize;

    for name in &sources {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let src = {
            let mut f = zip.by_name(name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            nvre_util::cp1252::decode(&b)
        };
        let Ok(g) = archive
            .extract_by_path(&format!("scripts\\{leaf}.pex"))
            .map_err(|e| e.to_string())
            .and_then(|b| PexFile::parse(&b).map_err(|e| e.to_string()))
        else {
            continue;
        };
        let Ok(script) = parse(&lex(&src)) else {
            continue;
        };
        let symbols = Symbols {
            script: script.name.clone(),
            parent: script.parent.clone().unwrap_or_default(),
            variables: Vec::new(),
            properties: Vec::new(),
            states: Vec::new(),
        };
        let Ok(c) = Compiler::new(&script, symbols).compile() else {
            continue;
        };

        for gstate in g.objects.iter().flat_map(|o| o.states.iter()) {
            let sname = g.text_or_empty(gstate.name).to_string();
            for gf in &gstate.functions {
                let gname = g.text_or_empty(gf.name).to_string();
                let have = c.states.iter().flat_map(|(_, fs)| fs.iter()).any(|f| {
                    c.strings.get(f.name as usize).map(String::as_str) == Some(gname.as_str())
                });
                if have {
                    continue;
                }
                total += 1;
                *by_name.entry(gname.to_ascii_lowercase()).or_default() += 1;
                // Is the function actually absent, or present under a different
                // spelling? Papyrus is case-insensitive, so a case-only mismatch
                // is a real defect — but a completely different one from a
                // function that is not emitted at all, and they need separating
                // before either can be acted on.
                let case_only = c.states.iter().flat_map(|(_, fs)| fs.iter()).any(|f| {
                    c.strings
                        .get(f.name as usize)
                        .map(|s| s.eq_ignore_ascii_case(&gname))
                        .unwrap_or(false)
                });
                if case_only {
                    *case_mismatch.entry(gname.clone()).or_default() += 1;
                    continue;
                }
                if rows.len() < limit {
                    // Enough of the source to see what form it is.
                    let decl = src
                        .lines()
                        .find(|l| {
                            let low = l.to_ascii_lowercase();
                            low.contains(&gname.to_ascii_lowercase())
                                && (low.contains("function") || low.contains("event"))
                        })
                        .map(|l| l.trim().to_string())
                        .unwrap_or_else(|| "<no declaration line found>".into());
                    rows.push(format!(
                        "{leaf} | state {sname:?} | {gname} | {}",
                        decl.chars().take(90).collect::<String>()
                    ));
                }
            }
        }
    }

    println!("missing functions: {total}\n");
    let ncase: usize = case_mismatch.values().sum();
    println!("  of which present under a DIFFERENT SPELLING: {ncase}");
    for (n, c) in &case_mismatch {
        println!("     {c:3}  {n}");
    }
    println!();
    println!("most common names:");
    let mut v: Vec<_> = by_name.into_iter().collect();
    v.sort_by_key(|r| std::cmp::Reverse(r.1));
    for (n, c) in v.iter().take(12) {
        println!("  {c:4}  {n}");
    }
    println!("\nfirst {limit} in script order:");
    for r in rows {
        println!("{r}\n");
    }
}
