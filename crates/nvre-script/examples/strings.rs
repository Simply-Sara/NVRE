//! Probe: how the game writes a **string argument**.
//!
//! Scans every `SCDA` in the master for instruction arguments of the shape
//! `[count:u16] [len:u16] <ascii>` and prints the count, the declared length,
//! the characters, and the bytes left over after them — the padding rule is
//! what the corpus says it is, not what it looks like from one example.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example strings
//! ```
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA");
        return;
    };
    let path = std::path::Path::new(&data).join("FalloutNV.esm");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("cannot read {}", path.display());
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), "FalloutNV.esm".into()) else {
        eprintln!("cannot parse");
        return;
    };

    let mut rows = 0usize;
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
        let mut at = 0usize;
        for ins in read_instructions(&compiled.data) {
            at += 1;
            let a = &ins.args;
            if a.len() < 6 {
                continue;
            }
            let count = u16::from_le_bytes([a[0], a[1]]) as usize;
            let len = u16::from_le_bytes([a[2], a[3]]) as usize;
            if count == 0 || len == 0 || len > 120 || 4 + len > a.len() {
                continue;
            }
            let s = &a[4..4 + len];
            if !s.iter().all(|b| *b == 0 || (0x20..0x7f).contains(b)) {
                continue;
            }
            let text: String = s.iter().map(|b| *b as char).collect();
            let tail = &a[4 + len..];
            let name = nvre_script::opcodes::command_at(ins.opcode)
                .map(|c| c.name)
                .unwrap_or("");
            rows += 1;
            println!(
                "{edid:<44} [{at:>3}] {:#06X} {name:<28} count={count} len={len:>2} chars={:>2} tail={:02X?} text={text:?}",
                ins.opcode,
                text.trim_end_matches('\0').len(),
                tail
            );
        }
    });
    println!("{rows} string-shaped arguments");
}
