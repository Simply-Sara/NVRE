//! The checker's acceptance criterion: **zero false positives on known-good code.**
//!
//! Bethesda's 14,301 scripts compile and ship. Any diagnostic
//! [`check`](nvre_papyrus::check::check) reports against them is a bug in the
//! checker, not in the script — so this counts them, per check, and prints enough
//! of each to explain.
//!
//! A check that is useful but noisy is worse than no check: the user learns to
//! ignore the output, and the one real error goes with it. So the rule is that
//! every finding here must be either zero or a script that genuinely does not
//! compile.
//!
//! ```text
//! cargo run --release --example checkcensus -- Scripts.zip
//! ```

use nvre_papyrus::check::Severity;
use nvre_papyrus::check::{check, Context};
use nvre_papyrus::symbols::ClassTable;
use nvre_papyrus::{lex, parse};
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

    // Every source, so inherited members resolve. This is the shape an editor
    // uses: the whole mod in one table.
    let mut parsed = Vec::new();
    for name in &names {
        let src = {
            let mut f = zip.by_name(name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            nvre_util::cp1252::decode(&b)
        };
        match parse(&lex(&src)) {
            Ok(s) => parsed.push((name.clone(), s)),
            Err(e) => println!("PARSE FAILED {name}: {e}"),
        }
    }
    let table = ClassTable::new(parsed.iter().map(|(_, s)| s));

    let mut scripts = 0usize;
    let mut scripts_with_findings = 0usize;
    let mut errors = 0usize;
    let mut warnings = 0usize;
    // Bucket by the check that fired, so the noisy one is identifiable.
    let mut buckets: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();

    for (name, script) in &parsed {
        scripts += 1;
        let ctx = Context::new(script, &table);
        let d = check(script, &ctx);
        if d.is_empty() {
            continue;
        }
        scripts_with_findings += 1;
        let leaf = name.rsplit('/').next().unwrap().to_string();
        for diag in &d {
            match diag.severity {
                Severity::Error => errors += 1,
                Severity::Warning => warnings += 1,
            }
            // The message names the symbol; strip it so equal kinds bucket
            // together.
            let key = if diag.message.starts_with("GotoState to") {
                "GotoState to an undeclared state".to_string()
            } else if diag.message.contains("takes at most") {
                "too many arguments".to_string()
            } else if diag.message.contains("is not declared here") {
                "unknown name".to_string()
            } else {
                diag.message.clone()
            };
            let e = buckets.entry(key).or_insert((0, Vec::new()));
            e.0 += 1;
            if e.1.len() < 8 {
                e.1.push(format!("{leaf}:{} {}", diag.line, diag.message));
            }
        }
    }

    println!("scripts checked: {scripts}");
    println!("scripts with any finding: {scripts_with_findings}");
    println!("  errors:   {errors}");
    println!("  warnings: {warnings}");
    println!();
    let mut v: Vec<_> = buckets.iter().collect();
    v.sort_by_key(|r| std::cmp::Reverse(r.1 .0));
    for (k, (n, ex)) in v {
        println!("{n:7}  {k}");
        for e in ex {
            println!("             {e}");
        }
    }
}
