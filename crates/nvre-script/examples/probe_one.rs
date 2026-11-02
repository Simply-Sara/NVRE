//! Probe: compile one named unit with its record's variable table, and print
//! the table beside the first differing bytes.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_one -- <EDID>
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

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
        .unwrap_or_else(|| "VFactionSquadListScript".into());

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

        println!("=== {edid} ({:08X}) ===", rec.header.form_id);
        println!("vars ({}):", vars.len());
        for v in &vars {
            println!("  id {:>3} {} int={}", v.id, v.name, v.is_int);
        }
        println!("scro ({}): {scro_names:?}", scro_names.len());
        match nvre_script::compile_with_refs(&source, &scro_names, &vars) {
            Ok(got) => {
                println!("compiled {} bytes, expected {}", got.len(), compiled.len());
                for (i, (a, b)) in got.iter().zip(compiled.iter()).enumerate() {
                    if a != b {
                        println!("first difference at {i}: got {a:02X} expected {b:02X}");
                        let lo = i.saturating_sub(8);
                        let hi = (i + 12).min(got.len()).min(compiled.len());
                        println!("  got      {:02X?}", &got[lo..hi]);
                        println!("  expected {:02X?}", &compiled[lo..hi]);
                        break;
                    }
                }
            }
            Err(e) => println!("compile error: {e}"),
        }
        println!();
        println!("--- source ---");
        for (n, line) in source.lines().enumerate() {
            println!("src {n:>3}| {line}");
            if n > 80 {
                break;
            }
        }
    });
}
