//! Probe: what arguments the corpus writes for a given opcode.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example opargs -- 1046 1213 1138
//! ```
//!
//! Prints, per opcode, how many corpus calls write each distinct argument byte
//! string, with an example unit for each — the way to settle an enum's codes
//! when the argument is a number and there is no name in the bytes to find.
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;
use std::collections::BTreeMap;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA");
        return;
    };
    let want: Vec<u16> = std::env::args()
        .skip(1)
        .filter_map(|a| u16::from_str_radix(a.trim_start_matches("0x"), 16).ok())
        .collect();
    if want.is_empty() {
        eprintln!("pass one or more hex opcodes");
        return;
    }
    let plugin_name = std::env::var("NVRE_PLUGIN").unwrap_or_else(|_| "FalloutNV.esm".to_string());
    let Ok(bytes) = std::fs::read(std::path::Path::new(&data).join(&plugin_name)) else {
        eprintln!("cannot read the master");
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), plugin_name) else {
        eprintln!("cannot parse");
        return;
    };
    // opcode -> (args -> (count, first unit))
    let mut census: BTreeMap<u16, BTreeMap<Vec<u8>, (usize, String)>> = BTreeMap::new();
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
        for ins in read_instructions(&compiled.data) {
            if want.contains(&ins.opcode) {
                let slot = census.entry(ins.opcode).or_default();
                let entry = slot.entry(ins.args.clone()).or_insert((0, edid.clone()));
                entry.0 += 1;
            }
            // An expression call is not a top-level instruction: it is the
            // `X` (0x58) token inside the enclosing instruction's arguments,
            // followed by `[opcode:u16][paramBytes:u16][count:u16][args…]`.
            // Scanning for that pair is the only way to census a call that
            // only ever appears inside an `if`.
            let a = &ins.args;
            let mut k = 0usize;
            while k + 7 <= a.len() {
                if a[k] == 0x58 {
                    let op = u16::from_le_bytes([a[k + 1], a[k + 2]]);
                    let param_bytes = u16::from_le_bytes([a[k + 3], a[k + 4]]) as usize;
                    let count = u16::from_le_bytes([a[k + 5], a[k + 6]]);
                    let end = k + 5 + param_bytes;
                    if want.contains(&op)
                        && param_bytes >= 2
                        && count <= 8
                        && end <= a.len()
                        && nvre_script::opcodes::command_at(op).is_some()
                    {
                        // Record the call's own bytes: the count, then its
                        // arguments — the same shape a statement call has.
                        let slot = census.entry(op).or_default();
                        let entry = slot
                            .entry(a[k + 5..end].to_vec())
                            .or_insert((0, edid.clone()));
                        entry.0 += 1;
                    }
                }
                k += 1;
            }
        }
    });
    for (op, args) in &census {
        let name = nvre_script::opcodes::command_at(*op)
            .map(|c| c.name)
            .unwrap_or("");
        let mut rows: Vec<(&Vec<u8>, &(usize, String))> = args.iter().collect();
        rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        println!(
            "{op:#06X} {name} — {} distinct argument strings",
            rows.len()
        );
        for (a, (n, unit)) in rows.iter().take(30) {
            println!("  {n:>5}  {:02X?}  ({unit})", a);
        }
    }
}
