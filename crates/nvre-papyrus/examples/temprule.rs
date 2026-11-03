//! Why does a function's first temporary have the number it has?
//!
//! Every attempt so far built a hypothesis from a handful of hand-written
//! `.psc` files and was then contradicted. This asks the corpus directly: for
//! each shipped function, record how many parameters it declares, how many the
//! body actually mentions, and the number of the FIRST `::tempN` it uses. Then
//! print the joint distribution, so the rule can be read off rather than fitted.
//!
//! ```text
//! cargo run --release --example temprule -- Scripts.zip "Misc.bsa"
//! ```

use nvre_papyrus::{PexFile, Value};
use std::collections::BTreeMap;

/// `::temp7` -> Some(7)
fn temp_index(name: &str) -> Option<u32> {
    name.strip_prefix("::temp")?.parse().ok()
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

    // (params, first_temp) -> count
    let mut joint: BTreeMap<(usize, u32), usize> = BTreeMap::new();
    let mut examples: BTreeMap<(usize, u32), String> = BTreeMap::new();
    // Restrict to functions with no default arguments and no locals, so the
    // only thing that could shift the counter is the parameter list.
    let mut clean: BTreeMap<(usize, u32), usize> = BTreeMap::new();
    let mut clean_ex: BTreeMap<(usize, u32), String> = BTreeMap::new();

    for name in &sources {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let Ok(bytes) = archive.extract_by_path(&format!("scripts\\{leaf}.pex")) else {
            continue;
        };
        let Ok(g) = PexFile::parse(&bytes) else {
            continue;
        };
        for st in g.objects.iter().flat_map(|o| o.states.iter()) {
            for f in &st.functions {
                let fname = g.text_or_empty(f.name).to_string();
                // Injected and native functions are not compiled from source.
                if f.flags & 2 != 0 || fname == "GetState" || fname == "GotoState" {
                    continue;
                }
                // First temporary this function mentions anywhere.
                let mut first: Option<u32> = None;
                for ins in &f.instructions {
                    for v in ins.args.iter().chain(ins.varargs.iter()) {
                        if let Value::Identifier(i) = v {
                            if let Some(n) = temp_index(g.text_or_empty(*i)) {
                                first = Some(first.map_or(n, |m| m.min(n)));
                            }
                        }
                    }
                }
                let Some(first) = first else { continue };
                let params = f.params.len();
                *joint.entry((params, first)).or_default() += 1;
                examples
                    .entry((params, first))
                    .or_insert_with(|| format!("{leaf}::{fname}"));

                // Locals here are temporaries only; a named local means the
                // function declares storage and is not a clean case.
                let named = f
                    .locals
                    .iter()
                    .any(|l| !g.text_or_empty(l.name).starts_with("::"));
                if !named {
                    *clean.entry((params, first)).or_default() += 1;
                    clean_ex
                        .entry((params, first))
                        .or_insert_with(|| format!("{leaf}::{fname}"));
                }
            }
        }
    }

    println!("params, first ::tempN, count — every shipped function");
    for ((p, t), n) in &joint {
        println!(
            "  {p} params -> temp{t:3}  x{n:6}   {}",
            examples[&(*p, *t)]
        );
    }

    println!();
    println!("the same, restricted to functions declaring no named local:");
    for ((p, t), n) in &clean {
        println!(
            "  {p} params -> temp{t:3}  x{n:6}   {}",
            clean_ex[&(*p, *t)]
        );
    }
}
