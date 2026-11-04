//! One shipped function, our stream beside the game's, aligned.
//!
//! ```text
//! cargo run --release --example whyfn -- \
//!   "<Data>/Scripts.zip" "<Data>/Skyrim - Misc.bsa" arenascript PickNextFight
//! ```
//!
//! The censuses say how many functions differ and bucket them; this says what
//! one of them actually is. Each line is `index  mine  |  game`, with the first
//! disagreeing index marked, and the locals table printed on both sides because
//! a wrong slot type shows up there first.
use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, opcodes, parse, PexFile, Value};

fn render(f: &PexFile, ins: &nvre_papyrus::pex::Instruction) -> String {
    let mut out = opcodes::name(ins.opcode).to_string();
    for v in ins.args.iter().chain(ins.varargs.iter()) {
        out.push(' ');
        out.push_str(&match v {
            Value::None => "None".to_string(),
            Value::Identifier(x) => f.text_or_empty(*x).to_string(),
            Value::String(x) => format!("{:?}", f.text_or_empty(*x)),
            Value::Int(n) => n.to_string(),
            Value::Float(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
        });
    }
    out
}

/// The same, against the compiler's own string table (a `Vec<String>`).
fn render_mine(strings: &[String], ins: &nvre_papyrus::pex::Instruction) -> String {
    let get = |i: u16| strings.get(i as usize).cloned().unwrap_or_default();
    let mut out = opcodes::name(ins.opcode).to_string();
    for v in ins.args.iter().chain(ins.varargs.iter()) {
        out.push(' ');
        out.push_str(&match v {
            Value::None => "None".to_string(),
            Value::Identifier(x) => get(*x),
            Value::String(x) => format!("{:?}", get(*x)),
            Value::Int(n) => n.to_string(),
            Value::Float(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
        });
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 {
        eprintln!("usage: whyfn <Scripts.zip> <archive.bsa> <script> [function]");
        return;
    }
    let want_func = a.get(4).cloned();

    // The source, out of the CK's own archive.
    let Ok(zip_bytes) = std::fs::read(&a[1]) else {
        eprintln!("cannot read {}", a[1]);
        return;
    };
    let Ok(mut zip) = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)) else {
        eprintln!("not a zip");
        return;
    };
    let needle = format!("scripts/{}.psc", a[3].to_ascii_lowercase());
    let mut source = None;
    for i in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(i) else {
            continue;
        };
        if entry.name().to_ascii_lowercase().replace('\\', "/").ends_with(&needle) {
            let mut buf = Vec::new();
            if std::io::Read::read_to_end(&mut entry, &mut buf).is_ok() {
                source = Some(nvre_util::cp1252::decode(&buf));
            }
            break;
        }
    }
    let Some(source) = source else {
        eprintln!("no source for {}", a[3]);
        return;
    };
    let script = match parse(&lex(&source)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("parse failed: {e}");
            return;
        }
    };
    let symbols = Symbols {
        script: script.name.clone(),
        parent: script.parent.clone().unwrap_or_default(),
        variables: Vec::new(),
        properties: Vec::new(),
        states: Vec::new(),
    };
    let mine = match Compiler::new(&script, symbols).compile() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("compile failed: {e}");
            return;
        }
    };

    // The shipped file.
    let Ok(mut archive) = bsa105::Archive::open(&a[2]) else {
        eprintln!("cannot open {}", a[2]);
        return;
    };
    let path = format!("scripts\\{}.pex", a[3].to_ascii_lowercase());
    let Ok(bytes) = archive.extract_by_path(&path) else {
        eprintln!("no .pex for {}", a[3]);
        return;
    };
    let Ok(game) = PexFile::parse(&bytes) else {
        eprintln!("the .pex did not parse");
        return;
    };

    for (state_name, funcs) in &mine.states {
        let sname = mine
            .strings
            .get(*state_name as usize)
            .cloned()
            .unwrap_or_default();
        for f in funcs {
            let fname = mine
                .strings
                .get(f.name as usize)
                .cloned()
                .unwrap_or_default();
            if want_func
                .as_deref()
                .is_some_and(|w| !w.eq_ignore_ascii_case(&fname))
            {
                continue;
            }
            // The game's side: same state, same name, case-insensitively.
            let mut theirs = None;
            for o in &game.objects {
                for st in &o.states {
                    if !game.text_or_empty(st.name).eq_ignore_ascii_case(&sname) {
                        continue;
                    }
                    for g in &st.functions {
                        if game.text_or_empty(g.name).eq_ignore_ascii_case(&fname) {
                            theirs = Some(g);
                        }
                    }
                }
            }
            println!("==== {sname}::{fname} ====");
            println!(
                "  mine locals {:?}",
                f.locals
                    .iter()
                    .map(|l| format!(
                        "{}:{}",
                        mine.strings.get(l.name as usize).map(String::as_str).unwrap_or(""),
                        mine.strings.get(l.type_name as usize).map(String::as_str).unwrap_or("")
                    ))
                    .collect::<Vec<_>>()
            );
            let Some(theirs) = theirs else {
                println!("  the game has no such function");
                println!();
                continue;
            };
            println!(
                "  game locals {:?}",
                theirs
                    .locals
                    .iter()
                    .map(|l| format!(
                        "{}:{}",
                        game.text_or_empty(l.name),
                        game.text_or_empty(l.type_name)
                    ))
                    .collect::<Vec<_>>()
            );
            let n = f.instructions.len().max(theirs.instructions.len());
            let mut first = true;
            for i in 0..n {
                let m = f.instructions.get(i).map(|x| render_mine(&mine.strings, x));
                let g = theirs.instructions.get(i).map(|x| render(&game, x));
                let mark = if m == g {
                    " "
                } else {
                    let mark = if first {
                        first = false;
                        ">"
                    } else {
                        "!"
                    };
                    mark
                };
                println!(
                    "{mark} {i:4} {:<48} | {}",
                    m.unwrap_or_default(),
                    g.unwrap_or_default()
                );
            }
            println!();
        }
    }
}
