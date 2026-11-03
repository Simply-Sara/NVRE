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
    let s = parse(&lex(&src)).unwrap();
    let sy = Symbols {
        script: s.name.clone(),
        parent: s.parent.clone().unwrap_or_default(),
        variables: Vec::new(),
        properties: Vec::new(),
        states: Vec::new(),
    };
    let c = Compiler::new(&s, sy).compile().unwrap();
    for (state, funcs) in &c.states {
        for f in funcs {
            println!(
                "fn {} lines={:?}",
                c.strings.get(f.name as usize).cloned().unwrap_or_default(),
                f.instructions
                    .iter()
                    .map(|i| NAMES[i.opcode as usize])
                    .collect::<Vec<_>>()
            );
        }
    }
    println!("\n--- per instruction (index: opcode), then lines:");
    for (_st, _f, _fun, lines) in &c.debug {
        let n = lines.len();
        println!("  {n} instructions, lines={lines:?}");
    }
    for (st, f, _fun, lines) in &c.debug {
        println!(
            "  {} {:25} lines={:?}",
            c.strings.get(*st as usize).cloned().unwrap_or_default(),
            c.strings.get(*f as usize).cloned().unwrap_or_default(),
            lines
        );
    }
}
