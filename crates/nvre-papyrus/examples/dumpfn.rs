use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, Value};

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
    for (_s, funcs) in &c.states {
        for f in funcs {
            let name = c.strings.get(f.name as usize).cloned().unwrap_or_default();
            if !name.eq_ignore_ascii_case("OnActivate") {
                continue;
            }
            println!(
                "--- mine {name}: {} instructions, {} locals",
                f.instructions.len(),
                f.locals.len()
            );
            for (i, ins) in f.instructions.iter().enumerate() {
                let args: Vec<String> = ins
                    .args
                    .iter()
                    .map(|x| match x {
                        Value::Identifier(k) => {
                            c.strings.get(*k as usize).cloned().unwrap_or_default()
                        }
                        Value::String(k) => format!(
                            "\"{}\"",
                            c.strings.get(*k as usize).cloned().unwrap_or_default()
                        ),
                        Value::Int(v) => v.to_string(),
                        Value::Bool(b) => b.to_string(),
                        Value::Float(_) => "f".into(),
                        o => format!("{o:?}"),
                    })
                    .collect();
                println!(
                    "  {i:2} {:12} {}",
                    NAMES[ins.opcode as usize],
                    args.join(" ")
                );
            }
            println!(
                "locals: {:?}",
                f.locals
                    .iter()
                    .map(|l| format!(
                        "{}:{}",
                        c.strings.get(l.name as usize).cloned().unwrap_or_default(),
                        c.strings
                            .get(l.type_name as usize)
                            .cloned()
                            .unwrap_or_default()
                    ))
                    .collect::<Vec<_>>()
            );
        }
    }
}
