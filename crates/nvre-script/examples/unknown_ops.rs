//! Probe: the opcodes the corpus writes that the table does not name.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example unknown_ops
//! ```
//!
//! Prints each unnamed opcode with how many corpus calls write it and one unit
//! each — the list of commands still missing from `opcodes::COMMANDS`, which is
//! the way to find one (`SystemSave`) whose opcode nothing else reveals.
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
    let plugin_name = std::env::var("NVRE_PLUGIN").unwrap_or_else(|_| "FalloutNV.esm".to_string());
    let Ok(bytes) = std::fs::read(std::path::Path::new(&data).join(&plugin_name)) else {
        eprintln!("cannot read the master");
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), plugin_name) else {
        eprintln!("cannot parse");
        return;
    };
    // opcode -> (count, first unit, example args)
    let mut census: BTreeMap<u16, (usize, String, Vec<u8>)> = BTreeMap::new();
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
            if nvre_script::opcodes::command_at(ins.opcode).is_some()
                || nvre_script::opcodes::Statement::is_statement(ins.opcode)
                || ins.opcode == 0x001C
            {
                continue;
            }
            let e = census
                .entry(ins.opcode)
                .or_insert((0, edid.clone(), ins.args.clone()));
            e.0 += 1;
        }
    });
    for (op, (n, unit, args)) in &census {
        println!("{op:#06X}  {n:>5}  ({unit})  {:02X?}", args);
    }
    println!("{} unnamed opcodes", census.len());
}
