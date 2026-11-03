//! Parse every `.psc` in the user's Skyrim `Scripts.zip` and report what the
//! parser makes of them: how many parse, and every file it cannot read with the
//! line and the token that stopped it.
//!
//! This is the compiler's RED measurement the same way `census` is the
//! reader's: a number to watch climb, over the game's own sources rather than
//! examples written to match the code.
//!
//! ```text
//! cargo run --release -p nvre-papyrus --example parse_census -- <Scripts.zip>
//! ```
use nvre_papyrus::{lex, parse};
use std::collections::BTreeMap;
use std::io::Read;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: parse_census <Scripts.zip>");
    let file = std::fs::File::open(&path).expect("the zip");
    let mut zip = zip::ZipArchive::new(file).expect("a zip");

    let mut names: Vec<String> = Vec::new();
    for i in 0..zip.len() {
        let n = zip.by_index(i).unwrap().name().to_string();
        if n.to_ascii_lowercase().ends_with(".psc") {
            names.push(n);
        }
    }
    names.sort();
    println!("{} sources", names.len());

    let mut ok = 0usize;
    let mut parse_failures: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: Vec<(String, String)> = Vec::new();
    let mut decls = 0usize;
    let mut functions = 0usize;
    let mut statements = 0usize;

    for name in &names {
        let s = {
            let mut f = zip.by_name(name).unwrap();
            // Sources are Windows-1252, not UTF-8.
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes).unwrap();
            nvre_util::cp1252::decode(&bytes)
        };
        let tokens = lex(&s);
        match parse(&tokens) {
            Ok(script) => {
                ok += 1;
                decls += script.decls.len();
                for d in &script.decls {
                    if let nvre_papyrus::parse::DeclKind::Function(f) = &d.kind {
                        functions += 1;
                        statements += f.body.len();
                    }
                }
            }
            Err(e) => {
                let msg = e.to_string();
                // The line the message names, and the source there, so a
                // failure can be read without opening the file.
                let line_no: usize = msg
                    .rsplit("line ")
                    .next()
                    .and_then(|x| x.trim().parse().ok())
                    .unwrap_or(0);
                let src_line = s
                    .lines()
                    .nth(line_no.saturating_sub(1))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let key = format!("{msg}   <<{src_line}>>");
                *parse_failures.entry(key.clone()).or_default() += 1;
                if examples.len() < 25 {
                    examples.push((name.clone(), key));
                }
            }
        }
    }

    println!("\nparsed {ok} / {}", names.len());
    println!("declarations {decls}, functions {functions}, top-level statements {statements}");
    println!("\nfailure reasons: {}", parse_failures.len());
    for (k, v) in &parse_failures {
        println!("  {v:5}  {k}");
    }
    println!("\nexamples:");
    for (n, e) in &examples {
        println!("  {n}: {e}");
    }
}
