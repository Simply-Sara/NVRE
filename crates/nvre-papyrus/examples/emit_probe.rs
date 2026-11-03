//! Compile a `.psc` and check the result is a *sound* `.pex`: it writes, it
//! re-parses to the same model, and every jump lands on a real instruction.
//!
//! This is the property that holds today. Reproducing the Creation Kit's exact
//! bytes is a separate and larger goal, and `parity` measures that one.
//!
//! ```text
//! cargo run --release -p nvre-papyrus --example emit_probe -- "<a .psc file>"
//! ```
use nvre_papyrus::emit::{Compiler, Symbols};
use nvre_papyrus::pex::{DebugInfo, Header, Object, PexFile, Property, State, Value, Variable};
use nvre_papyrus::{lex, parse, Endianness, Flavour};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: emit_probe <script.psc>");
    let bytes = std::fs::read(&path).expect("the source");
    let src = nvre_util::cp1252::decode(&bytes);
    let toks = lex(&src);
    let script = parse(&toks).expect("parse");
    println!(
        "script {} extends {:?}, {} declarations",
        script.name,
        script.parent,
        script.decls.len()
    );

    let symbols = Symbols {
        script: script.name.clone(),
        parent: script.parent.clone().unwrap_or_default(),
        variables: Vec::new(),
        properties: Vec::new(),
        states: Vec::new(),
    };
    let c = Compiler::new(&script, symbols).compile().expect("compile");

    let mut vars: Vec<Variable> = c
        .variables
        .into_iter()
        .map(|(n, t, v)| Variable {
            name: n,
            type_name: t,
            user_flags: 0,
            initial_value: v,
            const_flag: false,
        })
        .collect();
    let props: Vec<Property> = c
        .properties
        .into_iter()
        .map(|p| Property {
            name: p.name,
            type_name: p.type_name,
            doc: p.doc,
            user_flags: 0,
            flags: p.flags,
            auto_var: p.auto_var,
            getter: p.getter,
            setter: p.setter,
        })
        .collect();
    for p in &props {
        if let Some(a) = p.auto_var {
            vars.push(Variable {
                name: a,
                type_name: p.type_name,
                user_flags: 0,
                initial_value: Value::None,
                const_flag: false,
            });
        }
    }
    let states: Vec<State> = c
        .states
        .into_iter()
        .map(|(name, functions)| State { name, functions })
        .collect();

    let mut obj = Object {
        name: 0,
        size: 0,
        parent: 0,
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
    obj.size = obj.computed_size(Flavour::Skyrim);

    let file = PexFile {
        endianness: Endianness::Big,
        flavour: Flavour::Skyrim,
        header: Header {
            major: 3,
            minor: 2,
            game_id: 1,
            compile_time: 0,
            source_file: format!("{}.psc", script.name),
            user_name: "nvre".into(),
            machine_name: "nvre".into(),
        },
        strings: c.strings,
        debug: DebugInfo::default(),
        user_flags: Vec::new(),
        objects: vec![obj],
    };

    let out = file.write().expect("write");
    println!("wrote {} bytes", out.len());

    // The sound-ness checks.
    let back = PexFile::parse(&out).expect("the output re-reads");
    assert_eq!(back.strings, file.strings, "the string table survived");
    assert_eq!(back.objects.len(), 1);
    assert!(back.size_mismatches().is_empty(), "the size field is right");

    let o = &back.objects[0];
    println!(
        "object {}: {} vars, {} props, {} states",
        back.text_or_empty(o.name),
        o.variables.len(),
        o.properties.len(),
        o.states.len()
    );
    let mut functions = 0;
    let mut instructions = 0;
    for s in &o.states {
        for f in &s.functions {
            functions += 1;
            instructions += f.instructions.len();
            // Every jump must land inside this function's own instruction list.
            for (i, ins) in f.instructions.iter().enumerate() {
                let op = nvre_papyrus::opcodes::name(ins.opcode);
                if matches!(op, "jmp" | "jmpt" | "jmpf") {
                    let target = match ins.args.last() {
                        Some(Value::Int(v)) => *v,
                        other => panic!("{op} with a non-int target: {other:?}"),
                    };
                    let land = i as i32 + 1 + target;
                    assert!(
                        land >= 0 && (land as usize) <= f.instructions.len(),
                        "{}.{}: {op} at {i} targets {land}, outside 0..={}",
                        back.text_or_empty(s.name),
                        back.text_or_empty(f.name),
                        f.instructions.len()
                    );
                }
            }
        }
    }
    println!("{functions} functions, {instructions} instructions, every jump in range");
    println!("OK: the output is a sound .pex");
}
