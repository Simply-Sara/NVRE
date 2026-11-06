//! Probe: find the instruction that carries a given ASCII string.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example ascii -- "Times Addicted"
//! ```
//!
//! Prints the unit, the instruction's index, its opcode, and its raw argument
//! bytes — the way to read how the game encodes an argument whose shape the
//! emitter has not produced yet.
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA");
        return;
    };
    let Ok(needle) = std::env::args().nth(1).ok_or("pass the text to find") else {
        return;
    };
    let plugin_name = std::env::var("NVRE_PLUGIN").unwrap_or_else(|_| "FalloutNV.esm".to_string());
    let Ok(bytes) = std::fs::read(std::path::Path::new(&data).join(&plugin_name)) else {
        eprintln!("cannot read the master");
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), plugin_name) else {
        eprintln!("cannot parse");
        return;
    };
    let want = needle.as_bytes();
    let mut hits = 0usize;
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
        let Some(compiled) = subs.iter().find(|x| x.sig == *b"SCDA") else {
            return;
        };
        let ins = read_instructions(&compiled.data);
        for (i, one) in ins.iter().enumerate() {
            if !one.args.windows(want.len()).any(|w| w == want) {
                continue;
            }
            hits += 1;
            // The instruction alone, and the two around it for context.
            for j in i.saturating_sub(1)..(i + 2).min(ins.len()) {
                let name = nvre_script::opcodes::command_at(ins[j].opcode)
                    .map(|c| c.name)
                    .or_else(|| {
                        nvre_script::opcodes::Statement::from_opcode(ins[j].opcode)
                            .map(|s| s.keyword())
                    })
                    .unwrap_or("");
                let ascii: String = ins[j]
                    .args
                    .iter()
                    .map(|b| {
                        if (0x20..0x7f).contains(b) {
                            *b as char
                        } else {
                            '.'
                        }
                    })
                    .collect();
                println!(
                    "{edid:<40} [{j:>3}] {:#06X} {name:<22} len={:>2} {:02X?}  |{ascii}|",
                    ins[j].opcode,
                    ins[j].args.len(),
                    ins[j].args
                );
            }
            println!();
        }
    });
    println!("{hits} instructions carry {needle:?}");
}
