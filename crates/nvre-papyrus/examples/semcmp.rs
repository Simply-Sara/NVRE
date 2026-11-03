use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, PexFile, Value};
use std::collections::BTreeMap;

fn instrs(strings: &[String], f: &nvre_papyrus::pex::Function) -> Vec<String> {
    f.instructions
        .iter()
        .map(|i| {
            let a: Vec<String> = i
                .args
                .iter()
                .map(|x| match x {
                    Value::Identifier(k) => format!(
                        "id({})",
                        strings.get(*k as usize).map(|s| s.as_str()).unwrap_or("")
                    ),
                    Value::String(k) => format!(
                        "str({})",
                        strings.get(*k as usize).map(|s| s.as_str()).unwrap_or("")
                    ),
                    o => format!("{o:?}"),
                })
                .collect();
            format!("{:?} [{}] v{:?}", i.opcode, a.join(", "), i.varargs.len())
        })
        .collect()
}

fn body(strings: &[String], f: &nvre_papyrus::pex::Function) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "ret={} doc={} flags={} locals=[{}]\n",
        strings
            .get(f.return_type as usize)
            .cloned()
            .unwrap_or_default(),
        strings.get(f.doc as usize).cloned().unwrap_or_default(),
        f.flags,
        f.locals
            .iter()
            .map(|l| format!(
                "{}:{}",
                strings.get(l.name as usize).cloned().unwrap_or_default(),
                strings
                    .get(l.type_name as usize)
                    .cloned()
                    .unwrap_or_default()
            ))
            .collect::<Vec<_>>()
            .join(",")
    ));
    for i in instrs(strings, f) {
        s.push_str(&format!("  {i}\n"));
    }
    s
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = nvre_util::cp1252::decode(&std::fs::read(&a[1]).unwrap());
    let script = match parse(&lex(&src)) {
        Ok(s) => s,
        Err(_) => {
            println!("PARSE-FAIL");
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
    let c = match Compiler::new(&script, symbols).compile() {
        Ok(c) => c,
        Err(_) => {
            println!("EMIT-FAIL");
            return;
        }
    };
    let game = match PexFile::parse(&std::fs::read(&a[2]).unwrap()) {
        Ok(g) => g,
        Err(_) => {
            println!("GAME-FAIL");
            return;
        }
    };

    // mine, by function name
    let mut mine: BTreeMap<String, String> = BTreeMap::new();
    for (_sn, funcs) in &c.states {
        for f in funcs {
            mine.insert(
                c.strings
                    .get(f.name as usize)
                    .cloned()
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                body(&c.strings, f),
            );
        }
    }
    let mut theirs: BTreeMap<String, String> = BTreeMap::new();
    for o in &game.objects {
        for st in &o.states {
            for f in &st.functions {
                theirs.insert(
                    game.text_or_empty(f.name).to_ascii_lowercase(),
                    body(&game.strings, f),
                );
            }
        }
    }
    if mine == theirs {
        println!("SAME");
        return;
    }
    let mut shown = 0;
    for (k, v) in &mine {
        match theirs.get(k) {
            None => {
                println!("ONLY MINE: {k}");
                shown += 1;
            }
            Some(o) if o != v => {
                println!("DIFF {k}");
                let (x, y): (Vec<&str>, Vec<&str>) = (v.lines().collect(), o.lines().collect());
                for i in 0..x.len().max(y.len()) {
                    let (p, q) = (
                        x.get(i).copied().unwrap_or("<none>"),
                        y.get(i).copied().unwrap_or("<none>"),
                    );
                    if p != q {
                        println!("   mine: {p}\n   game: {q}");
                        shown += 1;
                        break;
                    }
                }
                shown += 1;
            }
            _ => {}
        }
        if shown >= 4 {
            break;
        }
    }
    for k in theirs.keys() {
        if !mine.contains_key(k) {
            println!("ONLY GAME: {k}");
            break;
        }
    }
}
