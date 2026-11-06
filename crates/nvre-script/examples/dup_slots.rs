//! Probe: which slot the game picks when a record names one form twice.
//!
//! ```text
//! NVRE_DATA=<Data> [NVRE_PLUGIN=Fallout3.esm] cargo run -p nvre-script --example dup_slots
//! ```
//!
//! A record's reference list can hold the same name in two slots, and the
//! compiler has to pick one. This prints, for every such unit, each instruction
//! that carries one of those slots, so the choice can be read off the bytes
//! instead of guessed.
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;

fn subs_of(rec: &nvre_esm::raw::Record) -> Vec<(String, Vec<u8>)> {
    let Ok(subs) = rec.subrecords() else {
        return Vec::new();
    };
    subs.iter()
        .map(|s| {
            (
                String::from_utf8_lossy(&s.sig).into_owned(),
                s.data.to_vec(),
            )
        })
        .collect()
}

fn text(d: &[u8]) -> String {
    String::from_utf8_lossy(d)
        .trim_end_matches('\0')
        .to_string()
}

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

    // FormID -> EDID, so an SCRO slot can be named.
    let mut edid_of: std::collections::HashMap<u32, String> = Default::default();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        for (sig, d) in subs_of(rec) {
            if sig == "EDID" {
                edid_of.insert(rec.header.form_id, text(&d));
            }
        }
    });
    edid_of.insert(0x14, "Player".to_string());

    let mut units = 0usize;
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let subs = subs_of(rec);
        let edid = subs
            .iter()
            .find(|(s, _)| s == "EDID")
            .map(|(_, d)| text(d))
            .unwrap_or_default();
        // The reference list in record order, named.
        let mut names: Vec<String> = Vec::new();
        let mut scro_fids: Vec<u32> = Vec::new();
        let mut vars: Vec<(u16, String)> = Vec::new();
        for (sig, d) in &subs {
            match sig.as_str() {
                "SCRO" if d.len() == 4 => {
                    let fid = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
                    scro_fids.push(fid);
                    names.push(
                        edid_of
                            .get(&fid)
                            .cloned()
                            .unwrap_or_else(|| format!("__form_{fid:08X}")),
                    );
                }
                "SCRV" if d.len() == 4 => {
                    let id = u16::from_le_bytes([d[0], d[1]]);
                    names.push(format!("__var_{id}"));
                }
                "SCVR" => vars.push((0, text(d))),
                "SLSD" if d.len() >= 20 => {
                    if let Some(last) = vars.last_mut() {
                        last.0 = u16::from_le_bytes([d[0], d[1]]);
                    }
                }
                _ => {}
            }
        }

        let _ = &mut vars;
        let dups: Vec<(String, Vec<usize>)> = {
            let mut out: Vec<(String, Vec<usize>)> = Vec::new();
            for n in names.iter() {
                if !out.iter().any(|(m, _)| m.eq_ignore_ascii_case(n)) {
                    let slots: Vec<usize> = names
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| m.eq_ignore_ascii_case(n))
                        .map(|(j, _)| j + 1)
                        .collect();
                    if slots.len() > 1 {
                        out.push((n.clone(), slots));
                    }
                }
            }
            out
        };
        if dups.is_empty() {
            return;
        }
        let Some((_, compiled)) = subs.iter().find(|(s, _)| s == "SCDA") else {
            return;
        };
        let Some((_, source)) = subs.iter().find(|(s, _)| s == "SCTX") else {
            return;
        };
        units += 1;
        println!("==== {edid}  refs {names:?}");
        for (name, slots) in &dups {
            println!(
                "  duplicate {name:?} at slots {slots:?} fids {:?}",
                slots
                    .iter()
                    .map(|s| format!("{:08X}", scro_fids.get(s - 1).copied().unwrap_or(0)))
                    .collect::<Vec<_>>()
            );
        }
        // Every instruction argument that carries one of the duplicated slots.
        let ins = read_instructions(compiled);
        for (i, one) in ins.iter().enumerate() {
            let a = &one.args;
            let mut k = 0usize;
            while k + 2 < a.len() {
                let tagged = matches!(a[k], 0x72 | 0x47 | 0x5A | 0x1C);
                if tagged {
                    let idx = u16::from_le_bytes([a[k + 1], a[k + 2]]);
                    if let Some((name, slots)) =
                        dups.iter().find(|(_, s)| s.contains(&(idx as usize)))
                    {
                        let which = if slots.first() == Some(&(idx as usize)) {
                            "first"
                        } else if slots.last() == Some(&(idx as usize)) {
                            "last"
                        } else {
                            "middle"
                        };
                        println!(
                            "    [{i:>3}] {:#06X} tag {:02X} idx {idx} = {name} ({which})",
                            one.opcode, a[k]
                        );
                    }
                }
                k += 1;
            }
        }
        for (n, line) in source
            .split(|b| *b == b'\n')
            .enumerate()
            .filter(|(_, l)| {
                let l = String::from_utf8_lossy(l).to_lowercase();
                dups.iter()
                    .any(|(name, _)| l.contains(&name.to_lowercase()))
            })
            .take(6)
        {
            println!(
                "    src {:>4}| {}",
                n + 1,
                String::from_utf8_lossy(line).trim_end_matches('\r')
            );
        }
        println!();
    });
    println!("{units} units name a form twice");
}
