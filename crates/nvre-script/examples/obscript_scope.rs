//! Probe: the obscript surface a compiler has to cover, measured over every
//! `SCPT` in `FalloutNV.esm`.
//!
//! Three questions, all answered from the game's own bytes rather than from a
//! spec written from memory:
//!
//! 1. Which statement opcodes appear in `SCDA` (0x10-0x1F)?
//! 2. Which command opcodes appear (0x1000+)?
//! 3. Which expression token types appear?
//!
//! The answer is the compiler's scope: a compiler that covers exactly what the
//! game itself compiles is a compiler that round-trips the whole corpus.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example obscript_scope
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use std::collections::BTreeMap;

fn u16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA to the game's Data folder");
        return;
    };
    let path = std::path::Path::new(&data).join("FalloutNV.esm");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("could not read {}", path.display());
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), "FalloutNV.esm".to_string()) else {
        eprintln!("could not parse the master");
        return;
    };

    let mut units: Vec<(u32, Vec<u8>)> = Vec::new();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
        if let Some(scda) = subs.iter().find(|x| x.sig == *b"SCDA") {
            if !scda.data.is_empty() {
                units.push((rec.header.form_id, scda.data.clone()));
            }
        }
    });
    println!("{} compiled scripts", units.len());

    let mut stmt_ops: BTreeMap<u16, usize> = BTreeMap::new();
    let mut cmd_ops: BTreeMap<u16, usize> = BTreeMap::new();
    let mut expr_toks: BTreeMap<u8, usize> = BTreeMap::new();
    let mut total_bytes: usize = 0;

    for (_, bytes) in &units {
        total_bytes += bytes.len();
        let mut i = 0usize;
        while i + 4 <= bytes.len() {
            let op = u16(bytes, i);
            let len = u16(bytes, i + 2) as usize;
            let args_start = i + 4;
            let args_end = (args_start + len).min(bytes.len());
            if (0x10..=0x1F).contains(&op) {
                *stmt_ops.entry(op).or_default() += 1;
            } else if op >= 0x1000 {
                *cmd_ops.entry(op).or_default() += 1;
            }
            for &b in &bytes[args_start..args_end] {
                if matches!(
                    b,
                    b's' | b'f' | b'G' | b'r' | b'n' | b'z' | b'X' | b'Z' | b' ' | b'~'
                ) {
                    *expr_toks.entry(b).or_default() += 1;
                }
            }
            i = args_end;
        }
    }

    println!("total compiled bytes {total_bytes}");
    println!();
    println!("statement opcodes ({} distinct):", stmt_ops.len());
    for (op, n) in &stmt_ops {
        println!("  0x{op:04X}  {n}");
    }
    println!();
    println!("command opcodes: {} distinct", cmd_ops.len());
    let lo = cmd_ops.keys().next().copied().unwrap_or(0);
    let hi = cmd_ops.keys().next_back().copied().unwrap_or(0);
    println!("  range 0x{lo:04X}..0x{hi:04X}");
    println!();
    println!("expression tag bytes:");
    for (t, n) in &expr_toks {
        println!("  {} {n}", *t as char);
    }
}
