//! Probe: pair every dotted reference use in a unit's source with the index
//! the game wrote for it, positionally.
//!
//! The reference-index space is the last unresolved rule, and deriving it
//! from one unit's bytes has produced contradictions — the pairing was
//! guessed, not read. This probe walks the unit's parsed statements and its
//! instructions in parallel (the way the corpus skill pairs them), and prints
//! each dotted use's source name beside the index the game wrote: the `1C`
//! statement prefix, the `72 ... 58` expression prefix, and the `72`-tagged
//! command arguments.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_pair_refs -- <EDID>
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;
use nvre_script::parse::{parse, Stmt};

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA");
        return;
    };
    let path = std::path::Path::new(&data).join("FalloutNV.esm");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("cannot read master");
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), "FalloutNV.esm".to_string()) else {
        eprintln!("cannot parse master");
        return;
    };

    let wanted = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "VFactionSquadCodeHolderScript".into());

    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        if !edid.eq_ignore_ascii_case(&wanted) {
            return;
        }
        let compiled = subs
            .iter()
            .find(|x| x.sig == *b"SCDA")
            .map(|x| x.data.clone())
            .unwrap_or_default();
        let source = subs
            .iter()
            .find(|x| x.sig == *b"SCTX")
            .map(|x| String::from_utf8_lossy(&x.data).into_owned())
            .unwrap_or_default();
        let scro: Vec<String> = subs
            .iter()
            .filter(|x| x.sig == *b"SCRO" && x.data.len() == 4)
            .map(|x| {
                let fid = u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]);
                format!("{fid:08X}")
            })
            .collect();
        let names: Vec<String> = subs
            .iter()
            .filter(|x| x.sig == *b"SCVR")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .collect();

        println!("=== {edid} ===");
        println!("SCRO ({}): {scro:?}", scro.len());
        println!("SLSD/SCVR ({}): {names:?}", names.len());

        // The source's dotted uses, in statement order: the parser's
        // Call statements with `on`, and the expressions inside If/Set.
        // Expressions are token runs; a dotted use there is Word '.' Word.
        let script = parse(&source);
        println!("--- dotted uses in source order ---");
        for (n, stmt) in script.stmts.iter().enumerate() {
            match stmt {
                Stmt::Call {
                    on: Some(r),
                    command,
                    ..
                } => {
                    println!("  stmt[{n}] dotted-call {r}.{command}");
                }
                Stmt::Set {
                    on: Some(r),
                    target,
                    ..
                } => {
                    println!("  stmt[{n}] dotted-set {r}.{target}");
                }
                Stmt::If(cond) | Stmt::ElseIf(cond) => {
                    // Word '.' Word triples in the condition
                    let words: Vec<&nvre_script::lex::Token> = cond
                        .iter()
                        .filter(|t| t.kind != nvre_script::lex::Kind::Space)
                        .collect();
                    for w in 0..words.len().saturating_sub(2) {
                        if words[w + 1].text == "." {
                            println!(
                                "  stmt[{n}] expr-dotted {}.{}",
                                words[w].text,
                                words[w + 2].text
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        println!("--- the game's ref-carrying indices, in bytecode order ---");
        let ins = read_instructions(&compiled);
        for (k, x) in ins.iter().enumerate() {
            if x.opcode == 0x001C {
                let idx = u16::from_le_bytes([x.args[0], x.args[1]]);
                // The next instruction is the command on this ref.
                let next = ins.get(k + 1).map(|y| y.opcode).unwrap_or(0);
                println!("  ins[{k}] 1C-prefix refIdx {idx} -> cmd 0x{next:04X}");
            }
            // A 72 before a 58 inside any instruction's args is an
            // expression dotted prefix.
            for w in x.args.windows(4).enumerate() {
                if w.1[0] == 0x72 && w.1[3] == 0x58 {
                    let idx = u16::from_le_bytes([w.1[1], w.1[2]]);
                    println!("  ins[{k}] @args+{} 72-expr-prefix refIdx {idx}", w.0);
                }
            }
        }
    });
}
