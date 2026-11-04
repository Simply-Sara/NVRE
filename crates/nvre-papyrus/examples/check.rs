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
use nvre_papyrus::symbols::ClassTable;
use nvre_papyrus::{lex, parse};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: check <file.psc>...");
        std::process::exit(2);
    }

    // Parse everything first: the class table needs the whole set before any
    // single script can be checked against it.
    let mut parsed: Vec<(String, nvre_papyrus::parse::Script)> = Vec::new();
    let mut parse_errors = 0usize;
    for path in &args {
        let src = match std::fs::read(path) {
            Ok(b) => nvre_util::cp1252::decode(&b),
            Err(e) => {
                println!("{path}:1: error: {e}");
                parse_errors += 1;
                continue;
            }
        };
        match parse(&lex(&src)) {
            Ok(s) => parsed.push((path.clone(), s)),
            Err(e) => {
                // A parse error is the diagnostic the user needs first: nothing
                // downstream can be trusted without it.
                println!("{path}:1: error: {e}");
                parse_errors += 1;
            }
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
