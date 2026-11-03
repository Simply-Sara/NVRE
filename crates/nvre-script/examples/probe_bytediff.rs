//! Probe: ours and the game's SCDA side by side, raw, around the first
//! divergence — for units where the instruction walker desyncs.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_bytediff -- <EDID>
//! ```
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        return;
    };
    let path = std::path::Path::new(&data).join("FalloutNV.esm");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), "f".into()) else {
        return;
    };
    let wanted = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "VEuclidQuestSCRIPT".into());
    // maps
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
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        formid_to_edid.insert(rec.header.form_id, edid.clone());
        if let Some(s) = subs.iter().find(|x| x.sig == *b"SCRI") {
            if s.data.len() == 4 {
                scri.insert(
                    rec.header.form_id,
                    u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]),
                );
            }
        }
        if matches!(sig.as_str(), "ACHR" | "REFR" | "ACRE") {
            if let Some(n) = subs.iter().find(|x| x.sig == *b"NAME") {
                if n.data.len() == 4 {
                    base_of.insert(
                        rec.header.form_id,
                        u32::from_le_bytes([n.data[0], n.data[1], n.data[2], n.data[3]]),
                    );
                }
            }
        }
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
                script_tables.insert(
                    edid,
                    names
                        .iter()
                        .zip(slcds.iter())
                        .map(|(n, d)| nvre_script::VarEntry {
                            name: n.clone(),
                            id: u16::from_le_bytes([d[0], d[1]]),
                            is_int: d[16] == 1,
                        })
                        .collect(),
                );
            }
        }
    });
    formid_to_edid.insert(0x14, "Player".into());
    sig_of.insert(0x14, "ACHR".into());
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
        let vars: Vec<nvre_script::VarEntry> = {
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
            names
                .iter()
                .zip(slcds.iter())
                .map(|(n, d)| nvre_script::VarEntry {
                    name: n.clone(),
                    id: u16::from_le_bytes([d[0], d[1]]),
                    is_int: d[16] == 1,
                })
                .collect()
        };
        let mut refs: Vec<nvre_script::RefEntry> = Vec::new();
        for s in subs.iter() {
            if s.sig == *b"SCRO" && s.data.len() == 4 {
                let fid = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                let name = formid_to_edid
                    .get(&fid)
                    .cloned()
                    .unwrap_or_else(|| format!("__form_{fid:08X}"));
                let kind = if sig_of.get(&fid).map(|x| x.as_str()) == Some("GLOB") {
                    nvre_script::RefKind::Global
                } else {
                    nvre_script::RefKind::Reference
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
        let got = nvre_script::compile_with_refs(&source, &refs, &vars, &remotes).unwrap();
        println!("ours {} bytes, game {} bytes", got.len(), compiled.len());
        // first differing byte
        let div = got
            .iter()
            .zip(compiled.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(usize::MAX);
        println!("first difference at {div}");
        let lo = div.saturating_sub(24);
        let hi = (div + 40).min(got.len()).min(compiled.len());
        println!("  ours    @{lo:>4} {:02X?}", &got[lo..hi]);
        for (n, chunk) in got.chunks(16).enumerate() {
            let at = n * 16;
            if at >= 380 && at < 560 {
                println!("  G @{at:>4} {:02X?}", chunk);
            }
        }
        println!("  game    @{lo:>4} {:02X?}", &compiled[lo..hi]);
        for (n, chunk) in compiled.chunks(16).enumerate() {
            let at = n * 16;
            if at >= 380 && at < 560 {
                println!("  E @{at:>4} {:02X?}", chunk);
            }
        }
        // the source lines, so the region can be read against them
        println!("--- source ---");
        for (n, l) in source.lines().enumerate() {
            println!("src{n:>3}| {l}");
        }
    });
}
