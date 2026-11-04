//! Is the **type environment** right, independent of instruction selection?
//!
//! A function's locals table is pure type information: every `::tempN` and its
//! declared type. It is produced by the same decisions that pick arithmetic
//! opcodes and insert `cast`s, but it contains no instructions — so comparing it
//! isolates the type model from every other emitter bug.
//!
//! The comparison is by **type multiset**, not by name: temporary numbering
//! drifts independently, so pairing `::temp3` with `::temp3` would measure the
//! allocator. Sorting both sides' type lists and comparing the counts asks one
//! question: did the same set of types get created?
//!
//! `::NoneVar` is dropped from both sides. It is one shared slot, not a
//! temporary, and it appears in nearly every function.
//!
//! ```text
//! cargo run --release --example typecensus -- Scripts.zip "Misc.bsa"
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, PexFile};
use std::collections::BTreeMap;
use std::io::Read;

/// The locals of one function as a sorted list of type names, `::NoneVar`
/// removed and case-folded so `Float` and `float` are the same type.
fn types_of(locals: &[(String, String)]) -> Vec<String> {
    let mut v: Vec<String> = locals
        .iter()
        .filter(|(n, _)| !n.eq_ignore_ascii_case("::NoneVar"))
        .map(|(_, t)| {
            let t = t.trim_end_matches("[]").to_ascii_lowercase();
            // A class name is stored however it was spelled; fold it.
            match t.as_str() {
                "int" | "integer" => "int".to_string(),
                "float" => "float".to_string(),
                "bool" => "bool".to_string(),
                "string" => "string".to_string(),
                "none" => "none".to_string(),
                other => other.to_string(),
            }
        })
        .collect();
    v.sort();
    v
}

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

    let mut total = 0usize;
    let mut exact = 0usize;
    // Which type does the game have that we do not, and vice versa?
    let mut extra_mine: BTreeMap<String, usize> = BTreeMap::new();
    let mut extra_game: BTreeMap<String, usize> = BTreeMap::new();
    // Functions whose type multiset is wrong, bucketed by the first difference.
    let mut buckets: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();

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
            let want_state = g.text_or_empty(gstate.name).to_ascii_lowercase();
            for gf in &gstate.functions {
                let gname = g.text_or_empty(gf.name).to_string();
                let mine = c
                    .states
                    .iter()
                    .find(|(s, _)| {
                        c.strings
                            .get(*s as usize)
                            .map(|n| n.to_ascii_lowercase() == want_state)
                            .unwrap_or(false)
                    })
                    .and_then(|(_, fs)| {
                        fs.iter().find(|f| {
                            c.strings.get(f.name as usize).map(String::as_str)
                                == Some(gname.as_str())
                        })
                    });
                let Some(m) = mine else { continue };
                total += 1;

                let gt: Vec<(String, String)> = gf
                    .locals
                    .iter()
                    .map(|t| {
                        (
                            g.text_or_empty(t.name).to_string(),
                            g.text_or_empty(t.type_name).to_string(),
                        )
                    })
                    .collect();
                let mt: Vec<(String, String)> = m
                    .locals
                    .iter()
                    .map(|t| {
                        (
                            c.strings.get(t.name as usize).cloned().unwrap_or_default(),
                            c.strings
                                .get(t.type_name as usize)
                                .cloned()
                                .unwrap_or_default(),
                        )
                    })
                    .collect();
                let gt = types_of(&gt);
                let mt = types_of(&mt);
                if gt == mt {
                    exact += 1;
                    continue;
                }

                // Tally the type-level difference.
                let mut gc: BTreeMap<&String, i32> = BTreeMap::new();
                for t in &gt {
                    *gc.entry(t).or_default() += 1;
                }
                for t in &mt {
                    *gc.entry(t).or_default() -= 1;
                }
                let mut key_parts = Vec::new();
                for (t, d) in &gc {
                    if *d > 0 {
                        *extra_game.entry((*t).clone()).or_default() += 1;
                        key_parts.push(format!("game+{t}"));
                    } else if *d < 0 {
                        *extra_mine.entry((*t).clone()).or_default() += 1;
                        key_parts.push(format!("mine+{t}"));
                    }
                }
                key_parts.sort();
                let key = key_parts.join(" ");
                *buckets.entry(key.clone()).or_default() += 1;
                let e = examples.entry(key).or_default();
                if e.len() < 3 {
                    e.push(format!("{leaf}::{gname}"));
                }
            }
        }
    }

    println!("functions paired: {total}");
    println!(
        "locals type multiset identical: {exact} ({:.1}%)",
        100.0 * exact as f64 / total as f64
    );
    println!();
    println!("the game declares these types and we do not:");
    let mut v: Vec<_> = extra_game.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    for (t, n) in v.iter().take(15) {
        println!("  {n:6}  {t}");
    }
    println!();
    println!("we declare these types and the game does not:");
    let mut v: Vec<_> = extra_mine.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    for (t, n) in v.iter().take(15) {
        println!("  {n:6}  {t}");
    }
    println!();
    println!("top buckets by first type difference:");
    let mut v: Vec<_> = buckets.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    for (k, n) in v.iter().take(14) {
        println!("  {n:6}  {k}");
        for e in examples.get(*k).map(|v| v.as_slice()).unwrap_or(&[]) {
            println!("            {e}");
        }
    }
}
