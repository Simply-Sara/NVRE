//! Content parity, with record order factored out.
//!
//! `semcensus` compares functions by name but still compares a script's
//! property list, variable list and state list **in file order**. The Creation
//! Kit stores those in hash containers and emits them in iteration order, which
//! is not decoded — so a script whose every field is right still reports as a
//! difference if its properties come out in a different sequence.
//!
//! This sorts every unordered collection before comparing, so what is left is
//! genuine content: same functions, same instructions, same operands, same
//! declared types. The gap between this number and `semcensus` is exactly the
//! cost of not knowing the record order, which is what makes it worth knowing
//! how far each piece of work gets.
//!
//! Usage: contentcensus <Scripts.zip> <Skyrim - Misc.bsa> [--show N]

use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, PexFile, Value};
use std::collections::BTreeMap;
use std::io::Read;

const NAMES: [&str; 36] = [
    "nop",
    "iadd",
    "fadd",
    "isub",
    "fsub",
    "imul",
    "fmul",
    "idiv",
    "fdiv",
    "imod",
    "not",
    "ineg",
    "fneg",
    "assign",
    "cast",
    "cmp_eq",
    "cmp_lt",
    "cmp_lte",
    "cmp_gt",
    "cmp_gte",
    "jmp",
    "jmpt",
    "jmpf",
    "callmethod",
    "callparent",
    "callstatic",
    "return",
    "strcat",
    "propget",
    "propset",
    "array_create",
    "array_length",
    "array_getelement",
    "array_setelement",
    "array_findelement",
    "array_rfindelement",
];

fn vname(strings: &[String], v: &Value) -> String {
    match v {
        Value::Identifier(i) => format!(
            "id({})",
            strings.get(*i as usize).cloned().unwrap_or_default()
        ),
        Value::String(i) => format!(
            "str({})",
            strings.get(*i as usize).cloned().unwrap_or_default()
        ),
        o => format!("{o:?}"),
    }
}

/// One function, canonically.
fn function(strings: &[String], f: &nvre_papyrus::pex::Function) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "{}:{}:{}:{}|",
        strings
            .get(f.name as usize)
            .cloned()
            .unwrap_or_default()
            .to_ascii_lowercase(),
        strings
            .get(f.return_type as usize)
            .cloned()
            .unwrap_or_default(),
        strings.get(f.doc as usize).cloned().unwrap_or_default(),
        f.flags
    ));
    let mut locals: Vec<String> = f
        .locals
        .iter()
        .map(|l| {
            format!(
                "{}:{}",
                strings.get(l.name as usize).cloned().unwrap_or_default(),
                strings
                    .get(l.type_name as usize)
                    .cloned()
                    .unwrap_or_default()
            )
        })
        .collect();
    locals.sort();
    s.push_str(&locals.join(","));
    s.push('\u{1}');
    for i in &f.instructions {
        s.push_str(&format!(
            "{}({})",
            NAMES.get(i.opcode as usize).copied().unwrap_or("?"),
            i.args
                .iter()
                .map(|a| vname(strings, a))
                .collect::<Vec<_>>()
                .join(",")
        ));
        if !i.varargs.is_empty() {
            s.push_str(&format!(
                "<{}>",
                i.varargs
                    .iter()
                    .map(|a| vname(strings, a))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        s.push(';');
    }
    s
}

fn prop(strings: &[String], p: &nvre_papyrus::pex::Property) -> String {
    let mut s = format!(
        "{}:{}:{}:{}:{}",
        strings
            .get(p.name as usize)
            .cloned()
            .unwrap_or_default()
            .to_ascii_lowercase(),
        strings
            .get(p.type_name as usize)
            .cloned()
            .unwrap_or_default(),
        strings.get(p.doc as usize).cloned().unwrap_or_default(),
        p.flags,
        p.auto_var
            .map(|a| strings
                .get(a as usize)
                .cloned()
                .unwrap_or_default()
                .to_ascii_lowercase())
            .unwrap_or_default()
    );
    for b in [p.getter.as_ref(), p.setter.as_ref()].into_iter().flatten() {
        s.push('\u{1}');
        s.push_str(&function(strings, b));
    }
    s
}

fn variable(strings: &[String], v: &nvre_papyrus::pex::Variable) -> String {
    format!(
        "{}:{}:{}:{}",
        strings
            .get(v.name as usize)
            .cloned()
            .unwrap_or_default()
            .to_ascii_lowercase(),
        strings
            .get(v.type_name as usize)
            .cloned()
            .unwrap_or_default(),
        v.user_flags,
        vname(strings, &v.initial_value)
    )
}

/// A whole script, canonically: every unordered list sorted.
fn canonical_game(f: &PexFile) -> String {
    let strings = &f.strings;
    let mut out = String::new();
    for o in &f.objects {
        out.push_str(&format!(
            "OBJ {} {} {} {} {}\u{1}",
            strings.get(o.name as usize).cloned().unwrap_or_default(),
            strings.get(o.parent as usize).cloned().unwrap_or_default(),
            strings.get(o.doc as usize).cloned().unwrap_or_default(),
            strings
                .get(o.auto_state as usize)
                .cloned()
                .unwrap_or_default(),
            o.user_flags
        ));
        let mut vs: Vec<String> = o.variables.iter().map(|v| variable(strings, v)).collect();
        vs.sort();
        out.push_str(&vs.join(","));
        out.push('\u{1}');
        let mut ps: Vec<String> = o.properties.iter().map(|p| prop(strings, p)).collect();
        ps.sort();
        out.push_str(&ps.join(","));
        out.push('\u{1}');
        let mut sts: Vec<String> = o
            .states
            .iter()
            .map(|s| {
                let mut fs: Vec<String> = s
                    .functions
                    .iter()
                    .map(|fun| function(strings, fun))
                    .collect();
                fs.sort();
                format!(
                    "STATE {} [{}]",
                    strings
                        .get(s.name as usize)
                        .cloned()
                        .unwrap_or_default()
                        .to_ascii_lowercase(),
                    fs.join(",")
                )
            })
            .collect();
        sts.sort();
        out.push_str(&sts.join(","));
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let show: usize = a
        .iter()
        .position(|x| x == "--show")
        .and_then(|i| a.get(i + 1))
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&a[1]).unwrap()).unwrap();
    let mut archive = bsa105::Archive::open(&a[2]).unwrap();
    let shipped: Vec<String> = archive
        .entries()
        .map(|e| e.path())
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            l.starts_with("scripts\\") && l.ends_with(".pex")
        })
        .collect();
    let mut sources = Vec::new();
    for i in 0..zip.len() {
        let n = zip.by_index(i).unwrap().name().to_string();
        if n.to_ascii_lowercase().ends_with(".psc") {
            sources.push(n);
        }
    }
    sources.sort();

    let (mut same, mut diff, mut fail, mut skip) = (0usize, 0usize, 0usize, 0usize);
    let mut shown = 0usize;
    // Why do the rest differ? Counted by the first field that disagrees, so the
    // answer is a ranked list rather than a sample.
    let mut why: BTreeMap<&'static str, usize> = BTreeMap::new();

    for name in &sources {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let shipped_path = format!("scripts\\{leaf}.pex");
        if !shipped
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&shipped_path))
        {
            skip += 1;
            continue;
        }
        let src = {
            let mut f = zip.by_name(name).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            nvre_util::cp1252::decode(&b)
        };
        let game = match archive
            .extract_by_path(&shipped_path)
            .map_err(|e| e.to_string())
            .and_then(|b| PexFile::parse(&b).map_err(|e| e.to_string()))
        {
            Ok(g) => g,
            Err(_) => {
                fail += 1;
                continue;
            }
        };
        let script = match parse(&lex(&src)) {
            Ok(s) => s,
            Err(_) => {
                fail += 1;
                continue;
            }
        };
        let symbols = Symbols {
            script: script.name.clone(),
            parent: script.parent.clone().unwrap_or_default(),
            variables: Vec::new(),
            properties: Vec::new(),
            states: Vec::new(),
        };
        let c = match Compiler::new(&script, symbols).compile() {
            Ok(c) => c,
            Err(_) => {
                fail += 1;
                continue;
            }
        };

        // The compiled script, in the same canonical shape.
        let strings = &c.strings;
        let s = |i: u16| strings.get(i as usize).cloned().unwrap_or_default();
        // The header fields come from the game's own table (its indices, its
        // strings); only the compiled values use `s`.
        let go = game.objects.first();
        let gs = |i: u16| game.text_or_empty(i).to_string();
        let mut mine = format!(
            "OBJ {} {} {} {} {}\u{1}",
            go.map(|o| gs(o.name)).unwrap_or_default(),
            go.map(|o| gs(o.parent)).unwrap_or_default(),
            go.map(|o| gs(o.doc)).unwrap_or_default(),
            go.map(|o| gs(o.auto_state)).unwrap_or_default(),
            go.map(|o| o.user_flags).unwrap_or(0)
        );
        let mut vs: Vec<String> = c
            .variables
            .iter()
            .map(|(n, t, v)| {
                format!(
                    "{}:{}:0:{}",
                    s(*n).to_ascii_lowercase(),
                    s(*t),
                    vname(strings, v)
                )
            })
            .collect();
        vs.sort();
        mine.push_str(&vs.join(","));
        mine.push('\u{1}');
        let mut ps: Vec<String> = c
            .properties
            .iter()
            .map(|p| {
                let mut m = format!(
                    "{}:{}:{}:{}:{}",
                    s(p.name).to_ascii_lowercase(),
                    s(p.type_name),
                    s(p.doc),
                    p.flags,
                    p.auto_var
                        .map(|a| s(a).to_ascii_lowercase())
                        .unwrap_or_default()
                );
                for b in [p.getter.as_ref(), p.setter.as_ref()].into_iter().flatten() {
                    m.push('\u{1}');
                    m.push_str(&function(strings, b));
                }
                m
            })
            .collect();
        ps.sort();
        mine.push_str(&ps.join(","));
        mine.push('\u{1}');
        let mut sts: Vec<String> = c
            .states
            .iter()
            .map(|(sn, funcs)| {
                let mut fs: Vec<String> = funcs.iter().map(|f| function(strings, f)).collect();
                fs.sort();
                format!("STATE {} [{}]", s(*sn).to_ascii_lowercase(), fs.join(","))
            })
            .collect();
        sts.sort();
        mine.push_str(&sts.join(","));

        let theirs = canonical_game(&game);
        if mine == theirs {
            same += 1;
        } else {
            diff += 1;
            {
                let mb: Vec<&str> = mine.split('\u{1}').collect();
                let tb: Vec<&str> = theirs.split('\u{1}').collect();
                let at = mb.iter().zip(tb.iter()).position(|(x, y)| x != y);
                let field = match at {
                    Some(0) => "object header (doc / auto-state / user flags)",
                    Some(1) => "variable list",
                    Some(2) => "property list",
                    Some(3) => "states and functions",
                    _ => "structure length",
                };
                *why.entry(field).or_default() += 1;
            }
            if shown < show {
                shown += 1;
                // first differing field group
                let mb: Vec<&str> = mine.split('\u{1}').collect();
                let tb: Vec<&str> = theirs.split('\u{1}').collect();
                let at = mb.iter().zip(tb.iter()).position(|(x, y)| x != y);
                let field = match at {
                    Some(0) => "object header",
                    Some(1) => "variables",
                    Some(2) => "properties",
                    Some(3) => "states/functions",
                    _ => "length",
                };
                println!("{leaf}: differs in {field}");
                if let Some(i) = at {
                    let (x, y) = (
                        mb.get(i).copied().unwrap_or(""),
                        tb.get(i).copied().unwrap_or(""),
                    );
                    let cx: Vec<&str> = x.split(',').collect();
                    let cy: Vec<&str> = y.split(',').collect();
                    for j in 0..cx.len().max(cy.len()) {
                        let (p, q) = (
                            cx.get(j).copied().unwrap_or("<none>"),
                            cy.get(j).copied().unwrap_or("<none>"),
                        );
                        if p != q {
                            println!("   mine: {}", &p[..p.len().min(150)]);
                            println!("   game: {}", &q[..q.len().min(150)]);
                            break;
                        }
                    }
                }
            }
        }
    }

    println!();
    println!("whay the rest differ, by first disagreeing field:");
    let mut ranked: Vec<(&str, usize)> = why.into_iter().collect();
    ranked.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (k, n) in &ranked {
        println!("  {n:6}  {k}");
    }
    println!();
    println!("content-identical scripts (order factored out): {same}");
    println!("still differing:                               {diff}");
    println!("parse/emit/archive failures:                   {fail}");
    println!("skipped (no shipped .pex):                     {skip}");
}
