//! Does the remaining 20% actually *behave* differently, or is it only numbering?
//!
//! Function-level parity counts `::tempN` differences, and a temporary number is
//! an internal detail: the VM resolves it through the function's own locals
//! table, so `::temp3` and `::temp7` with identical instructions around them
//! produce identical behaviour.
//!
//! This asks the question that matters for a playable mod: **would these two
//! functions do the same thing at run time?** It normalises temporaries by
//! first-use position rather than by number, then compares instruction by
//! instruction. A function that differs only by which slot a value landed in
//! comes out equal; one with a different opcode, a different operand, a different
//! branch target or a different length does not.
//!
//! ```text
//! cargo run --release --example behaviour -- Scripts.zip "Misc.bsa"
//! ```

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::Function;
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};
use std::collections::BTreeMap;
use std::io::Read;

/// Canonicalise a function's instructions for **behavioural** comparison.
///
/// Matching temporaries by name is too strict: `not ::temp18 ::temp18` and
/// `not ::temp20 ::temp19` do the same thing, because a destination slot's number
/// is immaterial as long as every read refers to the value its producer wrote.
///
/// So each argument is rewritten by where its value came from: a *destination*
/// argument becomes `$$` (which slot receives a value has no observable effect),
/// a read becomes `$d<n>` for the instruction that produced it, and a read of
/// something never written becomes `$u<k>` in order of first appearance.
///
/// Two functions that differ only in how the compiler numbered its scratch slots
/// then compare equal, while a different producer, operand, branch target or
/// length still differs.
fn normalise(lines: &[String]) -> Vec<String> {
    fn split(line: &str) -> (String, Vec<String>) {
        let mut parts = line.splitn(2, ' ');
        let op = parts.next().unwrap_or("").to_string();
        let rest = parts.next().unwrap_or("");
        let mut args = Vec::new();
        let mut cur = String::new();
        let mut in_str = false;
        for ch in rest.chars() {
            match ch {
                '"' => {
                    in_str = !in_str;
                    cur.push(ch);
                }
                ' ' if !in_str => {
                    if !cur.is_empty() {
                        args.push(std::mem::take(&mut cur));
                    }
                }
                _ => cur.push(ch),
            }
        }
        if !cur.is_empty() {
            args.push(cur);
        }
        (op, args)
    }

    /// Which argument position receives the result, if any.
    fn dest_index(op: &str) -> Option<usize> {
        match op {
            "callmethod" | "callparent" | "callstatic" | "propget" => Some(2),
            "assign" | "cast" | "cmp_eq" | "cmp_lt" | "cmp_lte" | "cmp_gt" | "cmp_gte" | "iadd"
            | "fadd" | "isub" | "fsub" | "imul" | "fmul" | "idiv" | "fdiv" | "imod" | "strcat"
            | "not" | "ineg" | "fneg" | "array_getelement" => Some(0),
            _ => None,
        }
    }

    // Pass 1: every instruction that writes a temporary, by name.
    let mut def_at: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, line) in lines.iter().enumerate() {
        let (op, args) = split(line);
        if let Some(d) = dest_index(&op) {
            if let Some(a) = args.get(d) {
                if a.starts_with("::temp") {
                    def_at.entry(a.clone()).or_insert_with(Vec::new).push(i);
                }
            }
        }
    }

    // Pass 2: rewrite argument by argument.
    let mut undef: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let (op, args) = split(line);
        let dest = dest_index(&op);
        let mut parts: Vec<String> = vec![op];
        for (pos, a) in args.iter().enumerate() {
            if Some(pos) == dest && a.starts_with("::temp") {
                parts.push("$$".to_string());
                continue;
            }
            if !a.starts_with("::temp") || a == "::NoneVar" {
                parts.push(a.clone());
                continue;
            }
            let digits: String = a[6..].chars().take_while(|c| c.is_ascii_digit()).collect();
            let name = format!("::temp{digits}");
            match def_at
                .get(&name)
                .and_then(|v| v.iter().rev().find(|&&d| d < i).copied())
            {
                Some(d) => parts.push(format!("$d{d}")),
                None => {
                    let next = undef.len();
                    let id = *undef.entry(name).or_insert(next);
                    parts.push(format!("$u{id}"));
                }
            }
        }
        out.push(parts.join(" "));
    }
    out
}

fn text(p: &PexFile, mine: Option<&[String]>, f: &Function) -> Vec<String> {
    let s = |i: u16| match mine {
        Some(t) => t.get(i as usize).cloned().unwrap_or_default(),
        None => p.text_or_empty(i).to_string(),
    };
    f.instructions
        .iter()
        .map(|i| {
            let mut out = opcodes::name(i.opcode).to_string();
            for a in i.args.iter().chain(i.varargs.iter()) {
                out.push(' ');
                out.push_str(&match a {
                    Value::None => "None".to_string(),
                    Value::Identifier(x) => s(*x),
                    Value::String(x) => format!("{:?}", s(*x)),
                    Value::Int(x) => x.to_string(),
                    Value::Float(x) => format!("{x}"),
                    Value::Bool(x) => x.to_string(),
                });
            }
            out
        })
        .collect()
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

    let (mut exact, mut numbering_only, mut behavioural) = (0usize, 0usize, 0usize);
    let mut funcs = 0usize;
    let mut why: BTreeMap<String, usize> = BTreeMap::new();
    let mut example: BTreeMap<String, String> = BTreeMap::new();
    let mut scripts_behavioural = 0usize;
    let mut scripts_total = 0usize;

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
        scripts_total += 1;
        let mut this_script_differs = false;

        for gstate in g.objects.iter().flat_map(|o| o.states.iter()) {
            for gf in &gstate.functions {
                let gname = g.text_or_empty(gf.name).to_string();
                // Match the STATE as well as the name: a function name can
                // appear in several states with different bodies, and taking the
                // first match silently compares the wrong pair — which showed up
                // as phantom "operand value on callmethod" differences.
                let want_state = g.text_or_empty(gstate.name).to_ascii_lowercase();
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
                funcs += 1;
                let Some(m) = mine else {
                    behavioural += 1;
                    this_script_differs = true;
                    *why.entry("function absent".into()).or_default() += 1;
                    continue;
                };
                let gt = text(&g, None, gf);
                let mt = text(&g, Some(&c.strings), m);
                if gt == mt {
                    exact += 1;
                    continue;
                }
                let gn = normalise(&gt);
                let mn = normalise(&mt);
                if gn == mn {
                    numbering_only += 1;
                    continue;
                }
                behavioural += 1;
                this_script_differs = true;
                // Classify from the NORMALISED text, so the bucket names the
                // real behavioural difference rather than a temp number.
                let k = match gn.iter().zip(&mn).position(|(x, y)| x != y) {
                    Some(n) => {
                        let gs: Vec<&str> = gn[n].split(' ').collect();
                        let ms: Vec<&str> = mn[n].split(' ').collect();
                        if gs[0] != ms[0] {
                            format!("different opcode: {} vs {}", ms[0], gs[0])
                        } else if gs.len() != ms.len() {
                            format!("operand count on {}", gs[0])
                        } else {
                            format!("operand value on {}", gs[0])
                        }
                    }
                    None => format!("length {} vs {}", mn.len(), gn.len()),
                };
                *why.entry(k.clone()).or_default() += 1;
                example
                    .entry(k)
                    .or_insert_with(|| format!("{leaf}::{gname}"));
            }
        }
        if this_script_differs {
            scripts_behavioural += 1;
        }
    }

    println!("functions: {funcs}");
    println!(
        "  identical, byte for byte:        {exact:6}  ({:.1}%)",
        100.0 * exact as f64 / funcs as f64
    );
    println!(
        "  identical up to temp NUMBERING:  {numbering_only:6}  ({:.1}%)",
        100.0 * numbering_only as f64 / funcs as f64
    );
    println!(
        "  BEHAVIOURALLY different:         {behavioural:6}  ({:.1}%)",
        100.0 * behavioural as f64 / funcs as f64
    );
    println!();
    println!(
        "scripts with at least one behavioural difference: {scripts_behavioural} / {scripts_total} ({:.1}%)",
        100.0 * scripts_behavioural as f64 / scripts_total as f64
    );
    println!();
    println!("what the behavioural differences are:");
    let mut v: Vec<_> = why.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    // Print every bucket, not the top 14: a truncated taxonomy ranks the work by
    // the first fourteen causes the corpus happened to hit, which is exactly the
    // trap of sampling when the question is "what is left".
    for (k, n) in v.iter() {
        println!(
            "  {n:6}  {k}   e.g. {}",
            example.get(k).map(String::as_str).unwrap_or("")
        );
    }
}
