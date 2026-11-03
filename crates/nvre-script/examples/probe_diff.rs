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
    let mut sig_of: std::collections::HashMap<u32, String> = Default::default();
    let mut scri: std::collections::HashMap<u32, u32> = Default::default();
    let mut base_of: std::collections::HashMap<u32, u32> = Default::default();
    let mut script_tables: std::collections::HashMap<String, Vec<nvre_script::VarEntry>> =
        Default::default();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        let Ok(subs) = rec.subrecords() else { return };
        let sig = String::from_utf8_lossy(&rec.header.sig).into_owned();
        sig_of.insert(rec.header.form_id, sig.clone());
        if let Some(edid_sub) = subs.iter().find(|x| x.sig == *b"EDID") {
            let edid = String::from_utf8_lossy(&edid_sub.data)
                .trim_end_matches('\0')
                .to_string();
            formid_to_edid.insert(rec.header.form_id, edid.clone());
            if rec.header.sig == *b"SCPT" {
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
                if names.len() == slcds.len() && !names.is_empty() {
                    let table: Vec<nvre_script::VarEntry> = names
                        .iter()
                        .zip(slcds.iter())
                        .map(|(n, d)| nvre_script::VarEntry {
                            name: n.clone(),
                            id: u16::from_le_bytes([d[0], d[1]]),
                            is_int: d[16] == 1,
                        })
                        .collect();
                    script_tables.insert(edid, table);
                }
            }
        }
        if let Some(s) = subs.iter().find(|x| x.sig == *b"SCRI") {
            if s.data.len() == 4 {
                scri.insert(
                    rec.header.form_id,
                    u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]),
                );
            }
        }
        if matches!(sig.as_str(), "ACHR" | "REFR" | "ACRE") {
            if let Some(name) = subs.iter().find(|x| x.sig == *b"NAME") {
                if name.data.len() == 4 {
                    base_of.insert(
                        rec.header.form_id,
                        u32::from_le_bytes([
                            name.data[0],
                            name.data[1],
                            name.data[2],
                            name.data[3],
                        ]),
                    );
                }
            }
        }
    });
    formid_to_edid.insert(0x00000014, "Player".to_string());
    sig_of.insert(0x00000014, "ACHR".to_string());

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
        let scro_fids: Vec<u32> = subs
            .iter()
            .filter(|x| x.sig == *b"SCRO" && x.data.len() == 4)
            .map(|x| u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]))
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

        // The reference list, record order: SCRO and SCRV interleaved.
        let mut refs: Vec<nvre_script::RefEntry> = Vec::new();
        for s in subs.iter() {
            if s.sig == *b"SCRO" && s.data.len() == 4 {
                let fid = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                let name = formid_to_edid
                    .get(&fid)
                    .cloned()
                    .unwrap_or_else(|| format!("__form_{fid:08X}"));
                let kind = match sig_of.get(&fid).map(|x| x.as_str()) {
                    Some("GLOB") => nvre_script::RefKind::Global,
                    _ => nvre_script::RefKind::Reference,
                };
                refs.push(nvre_script::RefEntry {
                    name,
                    slot: nvre_script::RefSlot::Form,
                    kind,
                    form: 0,
                });
            } else if s.sig == *b"SCRV" && s.data.len() == 4 {
                let var_id = u16::from_le_bytes([s.data[0], s.data[1]]);
                let name = vars
                    .iter()
                    .find(|v| v.id == var_id)
                    .map(|v| v.name.clone())
                    .unwrap_or_else(|| format!("__var_{var_id}"));
                refs.push(nvre_script::RefEntry {
                    name,
                    slot: nvre_script::RefSlot::Var,
                    kind: nvre_script::RefKind::Reference,
                    form: 0,
                });
            }
        }
        // The remote tables, through the SCRI / base chain.
        let mut remotes: Vec<nvre_script::RemoteScript> = Vec::new();
        for scro_fid in &scro_fids {
            let remote_name = formid_to_edid
                .get(scro_fid)
                .cloned()
                .unwrap_or_else(|| format!("__form_{scro_fid:08X}"));
            let script_fid = scri
                .get(scro_fid)
                .copied()
                .or_else(|| base_of.get(scro_fid).and_then(|b| scri.get(b).copied()));
            let Some(script_fid) = script_fid else {
                continue;
            };
            let Some(script_edid) = formid_to_edid.get(&script_fid).cloned() else {
                continue;
            };
            let Some(table) = script_tables.get(&script_edid).cloned() else {
                continue;
            };
            remotes.push(nvre_script::RemoteScript {
                name: remote_name,
                vars: table,
            });
        }

        let got = match nvre_script::compile_with_refs(&source, &refs, &vars, &remotes) {
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
