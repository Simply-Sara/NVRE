//! Probe: every opcode used inside an expression (`58` / `X` calls), with its
//! encoded parameters, and the source line that produced it.
//!
//! ```text
//! NVRE_DATA=<Data> [NVRE_PLUGIN=x.esm] cargo run -p nvre-script --example expr_ops -- 127C
//! ```
//!
//! A command that appears only in a condition is never a top-level instruction,
//! so `unknown_ops` cannot see it and the table entry cannot be read off the
//! statement form. This scans the `X` token inside every instruction's
//! arguments, prints the opcode and its parameter bytes, and pairs the call with
//! the source's own spelling of it — the same positional trick `pair_args` uses.
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
    let plugin_name = std::env::var("NVRE_PLUGIN").unwrap_or_else(|_| "FalloutNV.esm".to_string());
    let Ok(bytes) = std::fs::read(std::path::Path::new(&data).join(&plugin_name)) else {
        eprintln!("cannot read the master");
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), plugin_name) else {
        eprintln!("cannot parse");
        return;
    };

    // (opcode, params) -> (count, first unit)
    let mut census: BTreeMap<(u16, Vec<u8>), (usize, String)> = BTreeMap::new();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| String::from_utf8_lossy(&x.data).trim_end_matches('\0').to_string())
            .unwrap_or_default();
        let Some(compiled) = subs.iter().find(|x| x.sig == *b"SCDA") else {
            return;
        };
        for ins in read_instructions(&compiled.data) {
            let a = &ins.args;
            let mut k = 0usize;
            while k + 5 <= a.len() {
                if a[k] == 0x58 {
                    let op = u16::from_le_bytes([a[k + 1], a[k + 2]]);
                    let param_bytes = u16::from_le_bytes([a[k + 3], a[k + 4]]) as usize;
                    let end = k + 5 + param_bytes;
                    if end <= a.len() && (want.is_empty() || want.contains(&op)) {
                        let entry = census
                            .entry((op, a[k + 5..end].to_vec()))
                            .or_insert((0, edid.clone()));
                        entry.0 += 1;
                    }
                }
                k += 1;
            }
        }
    });

    // Group by opcode so the shapes of one command sit together.
    let mut by_op: BTreeMap<u16, Vec<(&Vec<u8>, &(usize, String))>> = BTreeMap::new();
    for (key, hit) in &census {
        by_op.entry(key.0).or_default().push((&key.1, hit));
    }
    for (op, rows) in by_op {
        let name = nvre_script::opcodes::command_at(op)
            .map(|c| c.name)
            .unwrap_or("<unnamed>");
        println!("{op:#06X} {name} — {} shape(s)", rows.len());
        for (params, (count, unit)) in rows.iter().take(6) {
            println!("  {count:>5}  params {params:02X?}  ({unit})");
        }
    }
}
