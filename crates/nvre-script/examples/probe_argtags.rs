//! Probe: per command, per parameter position, which value tag the game
//! writes for a numeric literal — `n`+i32 (0x6E) or `z`+f64 (0x7A).
//!
//! A command's table entry says its parameters' types; the corpus says how
//! each position is encoded. This walks every call, parses its arguments
//! structurally (skipping untagged raw-u16 ActorValue/AnimGroup positions),
//! and tabulates the tag at each position. A position that only ever sees
//! `z` declares a Float parameter, whatever the table's type string says.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_argtags
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;
use nvre_script::opcodes::COMMANDS;

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

    // opcode -> per-position tally: "n" / "z" / "r" / "raw" (untyped bytes)
    let mut tally: std::collections::BTreeMap<
        u16,
        Vec<std::collections::BTreeMap<&'static str, usize>>,
    > = std::collections::BTreeMap::new();

    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
        let compiled = subs
            .iter()
            .find(|x| x.sig == *b"SCDA")
            .map(|x| x.data.clone())
            .unwrap_or_default();
        if compiled.is_empty() {
            return;
        }
        let ins = read_instructions(&compiled);
        let mut k = 0usize;
        while k < ins.len() {
            let dotted = ins[k].opcode == 0x001C;
            let call = if dotted {
                if k + 1 >= ins.len() {
                    break;
                }
                &ins[k + 1]
            } else {
                &ins[k]
            };
            if call.opcode >= 0x1000 {
                let Some(cmd) = COMMANDS.iter().find(|c| c.opcode == call.opcode) else {
                    k += if dotted { 2 } else { 1 };
                    continue;
                };
                // Parse the call's arguments: [count:u16][args]. Each arg is
                // a tagged value, except positions the command declares as
                // ActorValue or AnimGroup, which are raw u16.
                if call.args.len() >= 2 {
                    let count = u16::from_le_bytes([call.args[0], call.args[1]]) as usize;
                    let mut p = 2usize;
                    let mut positions = Vec::new();
                    for pos in 0..count.min(cmd.args.len()) {
                        if p >= call.args.len() {
                            break;
                        }
                        let ty = cmd.args[pos];
                        if ty.eq_ignore_ascii_case("ActorValue")
                            || ty.eq_ignore_ascii_case("Actor Value")
                            || ty.eq_ignore_ascii_case("AnimGroup")
                        {
                            // raw u16
                            if p + 2 <= call.args.len() {
                                positions.push("raw");
                                p += 2;
                            }
                            continue;
                        }
                        let tag = call.args[p];
                        let label = match tag {
                            0x6E => "n",
                            0x7A => "z",
                            0x72 | 0x5A => "r",
                            0x73 | 0x66 => "v",
                            0x47 => "g",
                            _ => "?",
                        };
                        positions.push(label);
                        let step = match tag {
                            0x6E => 5,
                            0x7A => 9,
                            0x72 | 0x5A | 0x73 | 0x66 | 0x47 => 3,
                            _ => 1,
                        };
                        p += step;
                    }
                    let e = tally.entry(call.opcode).or_default();
                    while e.len() < positions.len() {
                        e.push(std::collections::BTreeMap::new());
                    }
                    for (i, label) in positions.iter().enumerate() {
                        *e[i].entry(label).or_insert(0) += 1;
                    }
                }
            }
            k += if dotted { 2 } else { 1 };
        }
    });

    let name_of = |op: u16| {
        COMMANDS
            .iter()
            .find(|c| c.opcode == op)
            .map(|c| c.name.to_string())
            .unwrap_or_else(|| "?".into())
    };
    let types_of = |op: u16| {
        COMMANDS
            .iter()
            .find(|c| c.opcode == op)
            .map(|c| c.args.join(", "))
            .unwrap_or_default()
    };

    // Report every command with a `z` anywhere, or a position where n and z
    // both appear (a table entry that cannot know the type from the source).
    for (op, positions) in &tally {
        let has_z = positions.iter().any(|m| m.contains_key("z"));
        let mixed = positions
            .iter()
            .any(|m| m.get("n").copied().unwrap_or(0) > 0 && m.get("z").copied().unwrap_or(0) > 0);
        if has_z || mixed {
            println!("0x{op:04X} {} [{}]", name_of(*op), types_of(*op));
            for (i, m) in positions.iter().enumerate() {
                let mut parts: Vec<String> = m.iter().map(|(k, v)| format!("{k}={v}")).collect();
                parts.sort();
                println!("    pos {}: {}", i, parts.join(" "));
            }
        }
    }
}
