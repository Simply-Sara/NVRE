use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, PexFile, Value};

/// Resolve a compiled string index against the compiler's own table.
fn mys(c: &[String], i: u16) -> &str {
    c.get(i as usize).map(|s| s.as_str()).unwrap_or("")
}
fn gs(f: &PexFile, i: u16) -> &str {
    f.text_or_empty(i)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = nvre_util::cp1252::decode(&std::fs::read(&a[1]).unwrap());
    let script = parse(&lex(&src)).unwrap();
    let symbols = Symbols {
        script: script.name.clone(),
        parent: script.parent.clone().unwrap_or_default(),
        variables: Vec::new(),
        properties: Vec::new(),
        states: Vec::new(),
    };
    let c = Compiler::new(&script, symbols).compile().unwrap();
    let game = PexFile::parse(&std::fs::read(&a[2]).unwrap()).unwrap();

    for (si, (sname, funcs)) in c.states.iter().enumerate() {
        println!("=== state {:?}", mys(&c.strings, *sname));
        let gstate = game.objects.get(0).and_then(|o| o.states.get(si));
        let gfuncs = gstate.map(|s| &s.functions[..]).unwrap_or(&[]);
        for (fi, f) in funcs.iter().enumerate() {
            println!("  FUNCTION {}", mys(&c.strings, f.name));
            if let Some(g) = gfuncs.get(fi) {
                println!("   game FUNCTION {}", gs(&game, g.name));
                let mine: Vec<String> = f
                    .instructions
                    .iter()
                    .map(|i| {
                        let args: Vec<String> = i
                            .args
                            .iter()
                            .map(|x| match x {
                                Value::Identifier(k) => format!("id({})", mys(&c.strings, *k)),
                                Value::String(k) => format!("str({})", mys(&c.strings, *k)),
                                o => format!("{o:?}"),
                            })
                            .collect();
                        format!("{:?} [{}]", i.opcode, args.join(", "))
                    })
                    .collect();
                let theirs: Vec<String> = g
                    .instructions
                    .iter()
                    .map(|i| {
                        let args: Vec<String> = i
                            .args
                            .iter()
                            .map(|x| match x {
                                Value::Identifier(k) => format!("id({})", gs(&game, *k)),
                                Value::String(k) => format!("str({})", gs(&game, *k)),
                                o => format!("{o:?}"),
                            })
                            .collect();
                        format!("{:?} [{}]", i.opcode, args.join(", "))
                    })
                    .collect();
                let n = mine.len().max(theirs.len());
                let mut shown = 0;
                for k in 0..n {
                    let x = mine.get(k).map(|s| s.as_str()).unwrap_or("<absent>");
                    let y = theirs.get(k).map(|s| s.as_str()).unwrap_or("<absent>");
                    if x != y {
                        println!("     {k:3} mine: {x}");
                        println!("         game: {y}");
                        shown += 1;
                        if shown >= 6 {
                            println!("     ...");
                            break;
                        }
                    }
                }
                if shown == 0 {
                    println!("     IDENTICAL ({} instrs)", mine.len());
                }
            }
        }
    }
}
