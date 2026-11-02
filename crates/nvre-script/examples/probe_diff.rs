//! Probe: dump the game's full instruction stream beside ours for one unit,
//! with the first divergence marked — for same-length-family bugs where the
//! difference is a few bytes somewhere in the middle.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_diff -- <EDID>
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
        .unwrap_or_else(|| "NVEcvDoorBG01SCRIPT".into());

    let mut formid_to_edid: std::collections::HashMap<u32, String> = Default::default();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        let Ok(subs) = rec.subrecords() else { return };
        if let Some(edid_sub) = subs.iter().find(|x| x.sig == *b"EDID") {
            formid_to_edid.insert(
                rec.header.form_id,
                String::from_utf8_lossy(&edid_sub.data)
                    .trim_end_matches('\0')
                    .to_string(),
            );
        }
    });
    formid_to_edid.insert(0x00000014, "Player".to_string());

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
        let scro_names: Vec<String> = subs
            .iter()
            .filter(|x| x.sig == *b"SCRO" && x.data.len() == 4)
            .map(|x| {
                let fid = u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]);
                formid_to_edid
                    .get(&fid)
                    .cloned()
                    .unwrap_or_else(|| format!("__form_{fid:08X}"))
            })
            .collect();
        let names: Vec<String> = subs
            .iter()
            .filter(|x| x.sig == *b"SCVR")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .collect();
        let slcds: Vec<&[u8]> = subs
            .iter()
            .filter(|x| x.sig == *b"SLSD" && x.data.len() >= 20)
            .map(|x| x.data.as_slice())
            .collect();
        let vars: Vec<nvre_script::VarEntry> = names
            .iter()
            .zip(slcds.iter())
            .map(|(name, d)| nvre_script::VarEntry {
                name: name.clone(),
                id: u16::from_le_bytes([d[0], d[1]]),
                is_int: d[16] == 1,
            })
            .collect();

        let got = match nvre_script::compile_with_refs(&source, &scro_names, &vars) {
            Ok(g) => g,
            Err(e) => {
                println!("compile error: {e}");
                return;
            }
        };
        println!(
            "=== {edid}: got {} bytes, expected {} ===",
            got.len(),
            compiled.len()
        );
        let gi = read_instructions(&got);
        let ei = read_instructions(&compiled);
        println!("got {} instructions, expected {}", gi.len(), ei.len());
        for (n, (g, e)) in gi.iter().zip(ei.iter()).enumerate() {
            let mark = if g == e { "  " } else { ">>" };
            println!(
                "{mark} [{n:>3}] got @{} 0x{:04X} {:02X?}   expected @{} 0x{:04X} {:02X?}",
                g.at, g.opcode, g.args, e.at, e.opcode, e.args
            );
            if g.at != e.at {
                println!("     (byte offsets diverge from here)");
            }
        }
        if gi.len() != ei.len() {
            println!("(instruction counts differ; tail of the longer stream:)");
            let longer = if gi.len() > ei.len() {
                &gi[..]
            } else {
                &ei[..]
            };
            for (n, x) in longer.iter().enumerate().skip(gi.len().min(ei.len())) {
                println!("     [{n:>3}] @{} 0x{:04X} {:02X?}", x.at, x.opcode, x.args);
            }
        }
    });
}
