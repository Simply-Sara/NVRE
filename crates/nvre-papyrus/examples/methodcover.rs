//! Would an "unknown method on a known receiver" check fire on known-good code?
//!
//! This is the feasibility probe for the next semantic check. The check is the
//! one that catches the most real mod bugs — `someActor.NotARealMethod()` compiles
//! today and fails in the game — but it can only ship if it produces **zero**
//! findings on Bethesda's own scripts.
//!
//! The risk is coverage, not correctness: `signatures` knows the methods the corpus
//! declared, so a method declared only on a class whose source was never published
//! would be flagged despite being real. So this measures, for every member call in
//! the corpus whose receiver class can be resolved:
//!
//!   * how many resolve to a known signature (the check would pass them),
//!   * how many do not (the check would flag them, and would be WRONG).
//!
//! Only if the second number is zero, or every case is explainable, is the check
//! safe to add.
//!
//! ```text
//! cargo run --release --example methodcover -- Scripts.zip
//! ```

use nvre_papyrus::parse::{DeclKind, Expr, FunctionDecl, Script, Stmt};
use nvre_papyrus::signatures;
use nvre_papyrus::symbols::ClassTable;
use nvre_papyrus::{lex, parse};
use std::collections::BTreeMap;
use std::io::Read;

/// Every member call in a body, with the class of its receiver when known.
fn walk<'a>(
    body: &'a [Stmt],
    script: &'a Script,
    table: &'a ClassTable,
    out: &mut Vec<(String, String, u16)>,
) {
    fn expr<'a>(
        e: &'a Expr,
        script: &'a Script,
        table: &'a ClassTable,
        out: &mut Vec<(String, String, u16)>,
        line: u16,
    ) {
        match e {
            Expr::Call { callee, args, .. } => {
                if let Expr::Member(obj, method) = callee.as_ref() {
                    if let Some(cls) = receiver_class(obj, script, table) {
                        out.push((cls, method.clone(), line));
                    }
                }
                if let Expr::Member(obj, _) = callee.as_ref() {
                    expr(obj, script, table, out, line);
                }
                for a in args {
                    expr(a, script, table, out, line);
                }
            }
            Expr::Member(obj, _) => expr(obj, script, table, out, line),
            Expr::Cast(inner, _) => expr(inner, script, table, out, line),
            Expr::Unary { operand, .. } => expr(operand, script, table, out, line),
            Expr::Binary { lhs, rhs, .. } => {
                expr(lhs, script, table, out, line);
                expr(rhs, script, table, out, line);
            }
            Expr::Index(a, b) => {
                expr(a, script, table, out, line);
                expr(b, script, table, out, line);
            }
            _ => {}
        }
    }

    for s in body {
        match s {
            Stmt::Local(l) => {
                if let Some(i) = &l.initial {
                    expr(i, script, table, out, l.line);
                }
            }
            Stmt::Assign {
                target,
                value,
                line,
            } => {
                expr(value, script, table, out, *line);
                expr(target, script, table, out, *line);
            }
            Stmt::CompoundAssign {
                target,
                value,
                line,
                ..
            } => {
                expr(value, script, table, out, *line);
                expr(target, script, table, out, *line);
            }
            Stmt::Expr(e, line) => expr(e, script, table, out, *line),
            Stmt::If { branches, .. } => {
                for b in branches {
                    if let Some(c) = &b.cond {
                        expr(c, script, table, out, b.line);
                    }
                    walk(&b.body, script, table, out);
                }
            }
            Stmt::While { cond, body, line } => {
                expr(cond, script, table, out, *line);
                walk(body, script, table, out);
            }
            Stmt::Return { value, line } => expr(value, script, table, out, *line),
            _ => {}
        }
    }
}

/// The class of a receiver expression, when it can be resolved.
fn receiver_class(e: &Expr, script: &Script, table: &ClassTable) -> Option<String> {
    match e {
        Expr::Ident(n) => {
            let lower = n.to_ascii_lowercase();
            if lower == "self" {
                return script.parent.clone();
            }
            if lower == "parent" {
                return script.parent.clone();
            }
            // A property or variable declared in this script, or inherited.
            for d in &script.decls {
                if d.name.eq_ignore_ascii_case(n) {
                    return Some(d.type_name.0.clone());
                }
            }
            script
                .parent
                .as_ref()
                .and_then(|p| table.member_type(p, n))
                .map(|t| t.0.clone())
        }
        Expr::Cast(_, ty) => Some(ty.0.clone()),
        _ => None,
    }
}

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

    let mut parsed = Vec::new();
    for name in &names {
        let src = {
            let mut f = zip.by_name(name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            nvre_util::cp1252::decode(&b)
        };
        if let Ok(s) = parse(&lex(&src)) {
            parsed.push((name.clone(), s));
        }
    }
    let table = ClassTable::new(parsed.iter().map(|(_, s)| s));

    let mut resolved = 0usize;
    let mut known = 0usize;
    let mut unknown: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    let mut unresolved_receiver = 0usize;

    for (name, script) in &parsed {
        let leaf = name.rsplit('/').next().unwrap().to_string();
        let mut calls = Vec::new();
        let mut collect = |f: &FunctionDecl| {
            walk(&f.body, script, &table, &mut calls);
        };
        for d in &script.decls {
            match &d.kind {
                DeclKind::Function(f) => collect(f),
                DeclKind::State(s) => {
                    for f in &s.functions {
                        collect(f);
                    }
                }
                _ => {}
            }
        }

        for (cls, method, line) in calls {
            if signatures::method_signature(&cls, &method).is_some()
                || signatures::method_signature(&cls.to_ascii_lowercase(), &method).is_some()
            {
                resolved += 1;
                known += 1;
            } else if signatures::return_type(&cls, &method).is_some() {
                // The return-type table is a second source: a method that appears
                // there is real even if its parameter shape was ambiguous.
                resolved += 1;
                known += 1;
            } else {
                resolved += 1;
                let key = format!(
                    "{}.{}",
                    cls.to_ascii_lowercase(),
                    method.to_ascii_lowercase()
                );
                let e = unknown.entry(key).or_insert((0, Vec::new()));
                e.0 += 1;
                if e.1.len() < 2 {
                    e.1.push(format!("{leaf}:{line}"));
                }
            }
        }
        let _ = &mut unresolved_receiver;
    }

    println!("member calls with a RESOLVED receiver class: {resolved}");
    println!(
        "  known to the tables:   {known} ({:.2}%)",
        100.0 * known as f64 / resolved.max(1) as f64
    );
    println!("  UNKNOWN (would be flagged): {}", resolved - known);
    println!();
    println!("distinct unknown class.method pairs: {}", unknown.len());
    let mut v: Vec<_> = unknown.iter().collect();
    v.sort_by_key(|r| std::cmp::Reverse(r.1 .0));
    for (k, (n, ex)) in v.iter() {
        println!("  {n:5}  {k}   e.g. {}", ex.join(", "));
    }
}
