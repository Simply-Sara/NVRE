//! Probe: dump the bytecode context around the first differing offset for a
//! named unit, plus the source, so a mismatched variable index can be paired
//! with the source line that produced it.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_unit
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
        eprintln!("cannot read master");
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), "FalloutNV.esm".to_string()) else {
        eprintln!("cannot parse master");
        return;
    };

    let wanted = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "VFactionSquadMasterScript".into());

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
        let names: Vec<String> = subs
            .iter()
            .filter(|x| x.sig == *b"SCVR")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .collect();

        println!(
            "=== {edid} ({:08X}), {} vars ===",
            rec.header.form_id,
            names.len()
        );
        for (n, line) in source.lines().enumerate() {
            println!("src {n:>3}| {line}");
        }
        println!("--- variable table (SLSD order) ---");
        for (n, v) in names.iter().enumerate() {
            println!("var {n:>2}: {v}");
        }
        println!("--- instructions ---");
        for ins in read_instructions(&compiled) {
            println!("@{:<5} 0x{:04X} args {:02X?}", ins.at, ins.opcode, ins.args);
        }
    });
}
