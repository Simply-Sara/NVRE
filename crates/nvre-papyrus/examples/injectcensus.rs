//! Are the "missing functions" reproducible from the shipped sources at all?
//!
//! A `.pex` should hold exactly the functions its source declares, plus the
//! injected `GetState` and `GotoState`. If some files hold more than that, the
//! question is whether a rule produces the extra ones or whether those `.pex`
//! files were built from source that was never shipped.
//!
//! This counts, per script: declarations in the source, and function slots in the
//! `.pex`. The difference is the injection count, and the distribution says which
//! of the two explanations holds.
//!
//! ```text
//! cargo run --release --example injectcensus -- Scripts.zip "Misc.bsa"
//! ```

use nvre_papyrus::parse::DeclKind;
use nvre_papyrus::{lex, parse, PexFile};
use std::collections::BTreeMap;
use std::io::Read;

fn main() {
    let a: Vec<String> = std::env::args().collect();
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

    // delta -> count, and a few examples of the unusual ones
    let mut deltas: BTreeMap<i64, usize> = BTreeMap::new();
    let mut examples: BTreeMap<i64, Vec<String>> = BTreeMap::new();

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

        // Declarations, counting functions inside states too.
        let mut declared = 0i64;
        for d in &script.decls {
            match &d.kind {
                DeclKind::Function(_) => declared += 1,
                DeclKind::State(st) => declared += st.functions.len() as i64,
                _ => {}
            }
        }
        let emitted: i64 = g
            .objects
            .iter()
            .flat_map(|o| o.states.iter())
            .map(|s| s.functions.len() as i64)
            .sum();

        // GetState and GotoState are always injected, so a correct file is +2.
        let delta = emitted - declared;
        *deltas.entry(delta).or_default() += 1;
        let e = examples.entry(delta).or_default();
        if e.len() < 4 {
            e.push(leaf);
        }
    }

    println!("pex functions minus source declarations (2 = GetState + GotoState):");
    for (d, n) in &deltas {
        println!(
            "  {d:+3}: {n:6} scripts   e.g. {}",
            examples.get(d).map(|v| v.join(", ")).unwrap_or_default()
        );
    }
}
