//! Where does the temporary counter diverge from the game's?
//!
//! The counter is script-wide and monotonic, so once it drifts every later
//! function in that script starts wrong. This walks each script's functions **in
//! file order** and prints the first temporary each one uses, game against mine.
//! The first function that disagrees is where the drift starts, and the cause is
//! in the function *before* it.
//!
//! ```text
//! cargo run --release --example tempdrift -- Scripts.zip "Misc.bsa" <script-name>
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, parse, PexFile, Value};
use std::io::Read;

fn first_temp(p: &PexFile, mine: Option<&[String]>, f: &Function) -> Option<u32> {
    let mut lo: Option<u32> = None;
    for ins in &f.instructions {
        for v in ins.args.iter().chain(ins.varargs.iter()) {
            if let Value::Identifier(i) = v {
                let name = match mine {
                    Some(t) => t.get(*i as usize).cloned().unwrap_or_default(),
                    None => p.text_or_empty(*i).to_string(),
                };
                if let Some(n) = name
                    .strip_prefix("::temp")
                    .and_then(|d| d.parse::<u32>().ok())
                {
                    lo = Some(lo.map_or(n, |m| m.min(n)));
                }
            }
        }
    }
    lo
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let want = a.get(3).map(|s| s.to_ascii_lowercase());

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

    let mut shown = 0;
    for name in &sources {
        if shown >= 3 {
            break;
        }
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_string();
        let low = leaf.to_ascii_lowercase();
        if let Some(w) = &want {
            if &low != w {
                continue;
            }
        }
        let src = {
            let mut f = zip.by_name(name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            nvre_util::cp1252::decode(&b)
        };
        let Ok(g) = archive
            .extract_by_path(&format!("scripts\\{low}.pex"))
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

        // The game's own bodies, in file order across all states.
        let mut rows = Vec::new();
        for st in g.objects.iter().flat_map(|o| o.states.iter()) {
            for gf in &st.functions {
                let gname = g.text_or_empty(gf.name).to_string();
                if gname == "GetState" || gname == "GotoState" {
                    continue;
                }
                let mine = c.states.iter().flat_map(|(_, fs)| fs.iter()).find(|f| {
                    c.strings.get(f.name as usize).map(String::as_str) == Some(gname.as_str())
                });
                let mine_first = mine.and_then(|m| first_temp(&g, Some(&c.strings), m));
                rows.push((gname, first_temp(&g, None, gf), mine_first));
            }
        }

        // Only report scripts that actually drift, and show the first few.
        if !rows.iter().any(|(_, g, m)| g != m) {
            continue;
        }
        println!("=== {leaf}");
        for (i, (n, gt, mt)) in rows.iter().enumerate() {
            let mark = if gt == mt { "  " } else { "!!" };
            println!("{mark}{i:3} {n:38} game {gt:?}  mine {mt:?}");
            if gt != mt && i > 0 {
                // The previous row is where the divergence was introduced.
                break;
            }
        }
        println!();
        shown += 1;
    }
}
