//! Probe: which commands does the game call WITHOUT writing the parameter
//! count?
//!
//! The measured rule is "a command that declares parameters writes a count
//! even when called with none" — but `rUser.StopCombat` compiles to
//! `1C 00 01 00 17 10 00 00`: StopCombat declares an Actor parameter in our
//! table, and the game wrote argLen 0. This probe walks every unit's
//! bytecode and tallies, per command opcode, calls written with a count vs
//! with argLen 0, and whether the argLen-0 ones are dotted (after a `1C`
//! prefix) or bare.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_zerocount
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

    // opcode -> name, for the commands our table says take parameters.
    let with_params: Vec<(u16, &str)> = COMMANDS
        .iter()
        .filter(|c| !c.args.is_empty())
        .map(|c| (c.opcode, c.name))
        .collect();

    // Per opcode: (with_count, without_count_dotted, without_count_bare)
    let mut tally: std::collections::BTreeMap<u16, (usize, usize, usize)> =
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
            if with_params.iter().any(|(op, _)| *op == call.opcode) {
                let has_count = call.args.len() >= 2;
                let e = tally.entry(call.opcode).or_insert((0, 0, 0));
                if has_count {
                    e.0 += 1;
                } else if dotted {
                    e.1 += 1;
                } else {
                    e.2 += 1;
                }
            }
            k += if dotted { 2 } else { 1 };
        }
    });

    println!("per-command tally (with_count / no_count_dotted / no_count_bare):");
    for (op, (yes, no_dotted, no_bare)) in &tally {
        if *no_dotted > 0 || *no_bare > 0 {
            let name = with_params
                .iter()
                .find(|(o, _)| o == op)
                .map(|(_, n)| *n)
                .unwrap_or("?");
            println!(
                "  0x{op:04X} {name:<28} with {yes:>5}  no-count dotted {no_dotted:>4}  bare {no_bare:>4}"
            );
        }
    }
    let total_yes: usize = tally.values().map(|(a, _, _)| a).sum();
    let total_no: usize = tally.values().map(|(_, b, c)| b + c).sum();
    println!("total: with count {total_yes}, without {total_no}");
}
