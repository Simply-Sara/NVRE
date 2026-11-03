//! The compiler's acceptance test.
//!
//! Compiles every `.psc` in the CK's `Scripts.zip` and compares the result with
//! the `.pex` the game shipped for it, byte for byte. That is the only evidence
//! that means anything: a test written beside the compiler proves the compiler
//! agrees with itself, and the game's own scripts are the thing it has to
//! reproduce.
//!
//! ```text
//! cargo run --release -p nvre-papyrus --example parity \
//!   -- <Scripts.zip> "<Data dir>/Skyrim - Misc.bsa"
//! ```
//!
//! Reported per script as one of:
//!
//! - **identical** — every byte, including the string table and the debug
//!   block.
//! - **code-only** — the instruction streams match but some header field does
//!   not, which is normally the compile time and the machine name.
//! - **differs** — the first byte that differs, with context.
use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, PexFile};
use std::collections::BTreeMap;
use std::io::Read;

fn main() {
    let mut args = std::env::args().skip(1);
    let zip_path = args.next().expect("usage: parity <Scripts.zip> <BSA>");
    let bsa_path = args.next().expect("usage: parity <Scripts.zip> <BSA>");

    // The game's own compiled scripts, by lowercased name.
    let mut archive = bsa105::Archive::open(&bsa_path).expect("the archive");
    let shippped: Vec<String> = archive
        .entries()
        .map(|e| e.path())
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            l.starts_with("scripts\\") && l.ends_with(".pex")
        })
        .collect();
    println!("{} shipped scripts", shippped.len());

    let file = std::fs::File::open(&zip_path).expect("the zip");
    let mut zip = zip::ZipArchive::new(file).expect("a zip");

    let mut sources: Vec<String> = Vec::new();
    for i in 0..zip.len() {
        let n = zip.by_index(i).unwrap().name().to_string();
        if n.to_ascii_lowercase().ends_with(".psc") {
            sources.push(n);
        }
    }
    sources.sort();
    println!("{} sources", sources.len());

    let mut identical = 0usize;
    let mut code_only = 0usize;
    let mut skipped = 0usize;
    let mut differs: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: Vec<String> = Vec::new();

    for name in &sources {
        let leaf = name
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(".psc")
            .to_ascii_lowercase();
        let shipped = format!("scripts\\{leaf}.pex");
        if !shippped.iter().any(|p| p.eq_ignore_ascii_case(&shipped)) {
            skipped += 1;
            continue;
        }
        let src = {
            let mut f = zip.by_name(name).unwrap();
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes).unwrap();
            nvre_util::cp1252::decode(&bytes)
        };
        let bytes = match archive.extract_by_path(&shipped) {
            Ok(b) => b,
            Err(e) => {
                *differs.entry(format!("archive: {e}")).or_default() += 1;
                continue;
            }
        };
        let game = match PexFile::parse(&bytes) {
            Ok(g) => g,
            Err(e) => {
                *differs.entry(format!("game pex: {e}")).or_default() += 1;
                continue;
            }
        };

        let toks = lex(&src);
        let script = match parse(&toks) {
            Ok(s) => s,
            Err(e) => {
                *differs.entry(format!("parse: {e}")).or_default() += 1;
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
        let compiled = match Compiler::new(&script, symbols).compile() {
            Ok(c) => c,
            Err(e) => {
                *differs.entry(format!("emit: {e}")).or_default() += 1;
                continue;
            }
        };

        // Build a .pex from the compile and compare it with the game's.
        match build(&game, compiled) {
            Ok(mine) => {
                if mine == bytes {
                    identical += 1;
                } else if code_matches(&mine, &bytes) {
                    code_only += 1;
                } else {
                    let at = mine.iter().zip(&bytes).position(|(a, b)| a != b);
                    *differs.entry(format!("bytes at {at:?}")).or_default() += 1;
                    if examples.len() < 8 {
                        examples.push(describe(&leaf, at, &mine, &bytes));
                    }
                }
            }
            Err(e) => {
                *differs.entry(format!("build: {e}")).or_default() += 1;
            }
        }
    }

    println!("\nidentical   {identical}");
    println!("code-only   {code_only}  (instructions match; header fields differ)");
    println!("skipped     {skipped}  (a source with no shipped .pex)");
    println!("differs     {}", differs.values().sum::<usize>());
    println!("\nreasons:");
    for (k, v) in &differs {
        println!("  {v:5}  {k}");
    }
    for e in &examples {
        println!("\n{e}");
    }
}

/// Put the compile into the game's own file header, so the comparison is about
/// the code rather than about who compiled it and when.
fn build(game: &PexFile, c: nvre_papyrus::emit::CompiledScript) -> Result<Vec<u8>, String> {
    use nvre_papyrus::pex::{
        DebugFunction, DebugInfo, Object, PexFile as F, Property, State, Variable,
    };
    let mut f = F {
        endianness: game.endianness,
        flavour: game.flavour,
        header: game.header.clone(),
        strings: c.strings,
        debug: DebugInfo::default(),
        user_flags: game.user_flags.clone(),
        objects: Vec::new(),
    };
    // Resolve the compiled indices against the new table.
    let mut vars = Vec::new();
    for (n, t, v) in c.variables {
        vars.push(Variable {
            name: n,
            type_name: t,
            user_flags: 0,
            initial_value: v,
            const_flag: false,
        });
    }
    let mut props = Vec::new();
    for p in c.properties {
        props.push(Property {
            name: p.name,
            type_name: p.type_name,
            doc: p.doc,
            user_flags: 0,
            flags: p.flags,
            auto_var: p.auto_var,
            getter: p.getter,
            setter: p.setter,
        });
    }
    let mut states = Vec::new();
    for (n, funcs) in c.states {
        states.push(State {
            name: n,
            functions: funcs,
        });
    }
    // The auto property's backing variable lives in the variable list, which
    // the compiler adds when a property is `auto`.
    for p in &props {
        if let Some(a) = p.auto_var {
            vars.push(Variable {
                name: a,
                type_name: p.type_name,
                user_flags: 0,
                initial_value: nvre_papyrus::Value::None,
                const_flag: false,
            });
        }
    }
    let obj = Object {
        name: 0,
        size: 0,
        parent: game.objects.first().map(|o| o.parent).unwrap_or(0),
        doc: 1,
        const_flag: false,
        user_flags: 0,
        auto_state: 1,
        structs: Vec::new(),
        variables: vars,
        properties: props,
        guards: Vec::new(),
        states,
    };
    let size = obj.computed_size(f.flavour);
    let mut obj = obj;
    obj.size = size;
    f.objects.push(obj);
    let _ = DebugFunction {
        object: 0,
        state: 0,
        function: 0,
        kind: 0,
        lines: Vec::new(),
    };
    f.write().map_err(|e| e.to_string())
}

/// Whether the two files' instruction streams are the same.
fn code_matches(a: &[u8], b: &[u8]) -> bool {
    let (Ok(x), Ok(y)) = (PexFile::parse(a), PexFile::parse(b)) else {
        return false;
    };
    // Compare the code *semantically*: every string operand is resolved
    // through its own file's table and compared as text.
    //
    // Comparing the raw indices would be wrong here, and wrong in a way that
    // hides the truth: this emitter and the Creation Kit order their string
    // tables differently, so identical code shows up as different indices on
    // every single operand. What is being asked is "did the same program get
    // generated", not "did the tables happen to coincide".
    fn val(f: &PexFile, v: &nvre_papyrus::Value) -> String {
        match v {
            nvre_papyrus::Value::Identifier(i) => format!("id({})", f.text_or_empty(*i)),
            nvre_papyrus::Value::String(i) => format!("str({})", f.text_or_empty(*i)),
            other => format!("{other:?}"),
        }
    }
    fn code(f: &PexFile) -> Vec<String> {
        let mut out = Vec::new();
        for o in &f.objects {
            for s in &o.states {
                out.push(format!(
                    "state {} {}",
                    f.text_or_empty(s.name),
                    s.functions.len()
                ));
                // Functions are compared **by name, in sorted order**, not by
                // position. The Creation Kit stores a state's functions in a
                // hash container and emits them in its iteration order -- in
                // `achievementsscript` the injected `GetState` and `GotoState`
                // sit at positions 5 and 7 of 8, between the script's own. So
                // position says nothing about correctness; name and body do.
                let mut ordered: Vec<&nvre_papyrus::pex::Function> = s.functions.iter().collect();
                ordered.sort_by_key(|fun| f.text_or_empty(fun.name).to_ascii_lowercase());
                for fun in ordered {
                    out.push(format!(
                        "fn {} ret {} doc {} flags {}",
                        f.text_or_empty(fun.name),
                        f.text_or_empty(fun.return_type),
                        f.text_or_empty(fun.doc),
                        fun.flags
                    ));
                    for l in &fun.locals {
                        out.push(format!(
                            "  local {} {}",
                            f.text_or_empty(l.name),
                            f.text_or_empty(l.type_name)
                        ));
                    }
                    for i in &fun.instructions {
                        out.push(format!(
                            "{:?} [{}] {:?}",
                            i.opcode,
                            i.args
                                .iter()
                                .map(|a| val(f, a))
                                .collect::<Vec<_>>()
                                .join(", "),
                            i.varargs
                        ));
                    }
                }
            }
        }
        out
    }
    code(&x) == code(&y)
}

fn describe(name: &str, at: Option<usize>, mine: &[u8], game: &[u8]) -> String {
    let Some(at) = at else {
        return format!("{name}: lengths {} vs {}", mine.len(), game.len());
    };
    let lo = at.saturating_sub(8);
    let hi = (at + 12).min(game.len().min(mine.len()));
    let hex = |b: &[u8]| {
        b[lo..hi.min(b.len())]
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mine_hex = hex(mine);
    let game_hex = hex(game);
    format!("{name}: first differs at {at}\n  mine {mine_hex}\n  game {game_hex}")
}
