//! Check `.psc` files and print diagnostics in `file:line: severity: message`
//! form, the shape an editor's problem pane wants.
//!
//! Build the class table from **every** `.psc` given, so a mod's own classes
//! resolve and inherited members are not reported as unknown. That is the point of
//! passing more than one file: one file alone cannot see its parent.
//!
//! ```text
//! cargo run --release --example check -- Mod/*.psc
//! ```
//!
//! Exit code is 1 when any error was reported, so it drops into a build step.

use nvre_papyrus::check::{check, Context, Severity};
use nvre_papyrus::lex;
use nvre_papyrus::parse::parse_partial;
use nvre_papyrus::symbols::ClassTable;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: check <file.psc>...");
        std::process::exit(2);
    }

    // Parse everything first: the class table needs the whole set before any
    // single script can be checked against it.
    //
    // A RECOVERY parse, not a strict one: a file that does not fully parse still
    // contributes the declarations that did, so the semantic checks run on them.
    // One error on line 6 must not hide an error on line 20.
    let mut parsed: Vec<(String, nvre_papyrus::parse::Script)> = Vec::new();
    let mut parse_errors = 0usize;
    for path in &args {
        let src = match std::fs::read(path) {
            Ok(b) => nvre_util::cp1252::decode(&b),
            Err(e) => {
                println!("{path}:1:1: error: {e}");
                parse_errors += 1;
                continue;
            }
        };
        let p = parse_partial(&lex(&src));
        for d in &p.errors {
            println!("{path}:{}:{}: error: {}", d.line, d.col, d.message);
            parse_errors += 1;
        }
        // Only keep the script if it has a name; a file with no `Scriptname` has
        // no AST worth checking.
        if !p.script.name.is_empty() {
            parsed.push((path.clone(), p.script));
        }
    }

    let table = ClassTable::new(parsed.iter().map(|(_, s)| s));

    let mut errors = 0usize;
    let mut warnings = 0usize;
    for (path, script) in &parsed {
        let ctx = Context::new(script, &table);
        for d in check(script, &ctx) {
            let sev = match d.severity {
                Severity::Error => {
                    errors += 1;
                    "error"
                }
                Severity::Warning => {
                    warnings += 1;
                    "warning"
                }
            };
            println!("{path}:{}: {sev}: {}", d.line, d.message);
        }
    }

    eprintln!(
        "\n{} file(s): {errors} error(s), {warnings} warning(s), {parse_errors} parse error(s)",
        args.len()
    );
    if errors + parse_errors > 0 {
        std::process::exit(1);
    }
}
