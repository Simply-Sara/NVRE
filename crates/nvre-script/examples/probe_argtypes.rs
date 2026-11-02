//! Probe: which commands encode a numeric argument as `z`+f64 vs `n`+i32?
//!
//! `emit_arg` writes the f64 form only when the command's declared parameter
//! type string is "Float", but the table spells some amount parameters
//! "Amount" or "Value". The corpus decides: for each command opcode, collect
//! every numeric literal argument the game wrote, and tabulate the tag it
//! used. A command whose literals are all `z` declares a Float parameter,
//! whatever the table says.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_argtypes
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

    // opcode -> (n_tag_count, z_tag_count)
    let mut tally: std::collections::BTreeMap<u16, (usize, usize)> =
        std::collections::BTreeMap::new();

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
            // A command call with a count: args follow [count:u16].
            if call.opcode >= 0x1000 && call.args.len() >= 2 {
                let count = u16::from_le_bytes([call.args[0], call.args[1]]) as usize;
                if count > 0 {
                    // Walk the count arguments. Each is a tagged value:
                    // n + i32 (5 bytes), r + u16 (3), z + f64 (9), 72 + u16 (3).
                    let mut p = 2usize;
                    for _ in 0..count {
                        if p >= call.args.len() {
                            break;
                        }
                        match call.args[p] {
                            0x6E => {
                                if p + 5 <= call.args.len() {
                                    let e = tally.entry(call.opcode).or_insert((0, 0));
                                    e.0 += 1;
                                }
                                p += 5;
                            }
                            0x7A => {
                                if p + 9 <= call.args.len() {
                                    let e = tally.entry(call.opcode).or_insert((0, 0));
                                    e.1 += 1;
                                }
                                p += 9;
                            }
                            0x72 | 0x5A => p += 3,
                            _ => break,
                        }
                    }
                }
            }
            k += if dotted { 2 } else { 1 };
        }
    });

    println!("opcode   name                          n-tag   z-tag   table-arg-types");
    for (op, (n, z)) in &tally {
        if *n == 0 && *z == 0 {
            continue;
        }
        let name = COMMANDS
            .iter()
            .find(|c| c.opcode == *op)
            .map(|c| c.name.to_string())
            .unwrap_or_else(|| "?".into());
        let types = COMMANDS
            .iter()
            .find(|c| c.opcode == *op)
            .map(|c| c.args.join(", "))
            .unwrap_or_default();
        // Only interesting: commands where the tag split says something the
        // table's type strings might not.
        let interesting = *z > 0 || *n > 0;
        if interesting {
            println!("0x{op:04X}   {name:<28} {n:>5}   {z:>5}   {types}");
        }
    }
}
