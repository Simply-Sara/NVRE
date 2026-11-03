use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::{lex, parse, PexFile, Value};
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
fn show(tag: &str, _strings: &[String], funcs: &[(String, Vec<String>, Vec<u16>)]) {
    for (name, ins, _lines) in funcs {
        println!("{tag} {name} ({} instrs)", ins.len());
        for (i, x) in ins.iter().enumerate() {
            println!("   {i:2} {x}");
        }
    }
}
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = nvre_util::cp1252::decode(&std::fs::read(&a[1]).unwrap());
    let s = parse(&lex(&src)).unwrap();
    let sy = Symbols {
        script: s.name.clone(),
        parent: s.parent.clone().unwrap_or_default(),
        variables: Vec::new(),
        properties: Vec::new(),
        states: Vec::new(),
    };
    let c = Compiler::new(&s, sy).compile().unwrap();
    let v = |st: &[String], x: &Value| match x {
        Value::Identifier(i) => st.get(*i as usize).cloned().unwrap_or_default(),
        Value::String(i) => format!("\"{}\"", st.get(*i as usize).cloned().unwrap_or_default()),
        o => format!("{o:?}"),
    };
    let mut mine = Vec::new();
    for (_sn, fs) in &c.states {
        for f in fs {
            let name = c.strings.get(f.name as usize).cloned().unwrap_or_default();
            let ins: Vec<String> = f
                .instructions
                .iter()
                .map(|i| {
                    format!(
                        "{}({})",
                        NAMES[i.opcode as usize],
                        i.args
                            .iter()
                            .map(|x| v(&c.strings, x))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect();
            mine.push((name, ins, Vec::new()));
        }
    }
    show("MINE", &c.strings, &mine);
    if let Some(p) = a.get(2) {
        let g = PexFile::parse(&std::fs::read(p).unwrap()).unwrap();
        let mut theirs = Vec::new();
        for o in &g.objects {
            for st in &o.states {
                for f in &st.functions {
                    let name = g.text_or_empty(f.name).to_string();
                    let ins: Vec<String> = f
                        .instructions
                        .iter()
                        .map(|i| {
                            format!(
                                "{}({})",
                                NAMES[i.opcode as usize],
                                i.args
                                    .iter()
                                    .map(|x| v(&g.strings, x))
                                    .collect::<Vec<_>>()
                                    .join(",")
                            )
                        })
                        .collect();
                    theirs.push((name, ins, Vec::new()));
                }
            }
        }
        show("GAME", &g.strings, &theirs);
    }
}
