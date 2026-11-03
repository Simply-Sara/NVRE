//! Probe: the `SCRI` chain from a named record to the script that runs on it,
//! and that script's variable table.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example scri_chain -- GomorrahCasinoEnterTriggerREF
//! ```
//!
//! The remote-variable form (`Script.var`) needs the *target script's* own
//! `SLSD` table, and the chain that reaches it is measured, not obvious: a
//! quest runs its script through its own `SCRI`, a placed reference through its
//! base object's. This prints each step so a table the compiler refuses for a
//! name that clearly exists can be traced to the link that is missing.
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA");
        return;
    };
    let Some(wanted) = std::env::args().nth(1) else {
        eprintln!("pass a record EDID");
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

    let mut by_edid: std::collections::HashMap<String, u32> = Default::default();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        let Ok(subs) = rec.subrecords() else { return };
        if let Some(e) = subs.iter().find(|x| x.sig == *b"EDID") {
            by_edid.insert(
                String::from_utf8_lossy(&e.data).trim_end_matches('\0').to_string(),
                rec.header.form_id,
            );
        }
    });
    let Some(&fid) = by_edid.get(&wanted) else {
        println!("no record named {wanted}");
        return;
    };

    let mut report = |id: u32, label: &str| {
        let mut found = false;
        visit_nodes(&plugin.nodes, &mut |rec, _| {
            if rec.header.form_id != id {
                return;
            }
            found = true;
            let Ok(subs) = rec.subrecords() else { return };
            let sig = String::from_utf8_lossy(&rec.header.sig).into_owned();
            let edid = subs
                .iter()
                .find(|x| x.sig == *b"EDID")
                .map(|x| String::from_utf8_lossy(&x.data).trim_end_matches('\0').to_string())
                .unwrap_or_default();
            println!(
                "{label} {id:08X} {sig} {edid}{}",
                if subs.iter().any(|x| x.sig == *b"SCRI") {
                    format!(
                        "  SCRI -> {:08X}",
                        {
                            let s = subs.iter().find(|x| x.sig == *b"SCRI").unwrap();
                            u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]])
                        }
                    )
                } else {
                    String::new()
                }
            );
            if let Some(s) = subs.iter().find(|x| x.sig == *b"SCRI") {
                if s.data.len() == 4 {
                    let script = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                    visit_nodes(&plugin.nodes, &mut |sc, _| {
                        if sc.header.form_id != script || sc.header.sig != *b"SCPT" {
                            return;
                        }
                        let Ok(ss) = sc.subrecords() else { return };
                        let names: Vec<String> = ss
                            .iter()
                            .filter(|x| x.sig == *b"SCVR")
                            .map(|x| {
                                String::from_utf8_lossy(&x.data)
                                    .trim_end_matches('\0')
                                    .to_string()
                            })
                            .collect();
                        let slsd: Vec<&[u8]> = ss
                            .iter()
                            .filter(|x| x.sig == *b"SLSD")
                            .map(|x| x.data.as_slice())
                            .collect();
                        println!(
                            "      script {script:08X} {} vars: {}",
                            ss.iter()
                                .find(|x| x.sig == *b"EDID")
                                .map(|x| String::from_utf8_lossy(&x.data)
                                    .trim_end_matches('\0')
                                    .to_string())
                                .unwrap_or_default(),
                            names
                                .iter()
                                .zip(slsd.iter())
                                .map(|(n, d)| format!(
                                    "{n}={}",
                                    u16::from_le_bytes([d[0], d[1]])
                                ))
                                .collect::<Vec<_>>()
                                .join(" ")
                        );
                        let _ = ss.iter().find(|x| x.sig == *b"SCDA");
                    });
                }
            }
            if matches!(&sig[..], "ACHR" | "REFR" | "ACRE") {
                if let Some(n) = subs.iter().find(|x| x.sig == *b"NAME") {
                    if n.data.len() == 4 {
                        let base = u32::from_le_bytes([n.data[0], n.data[1], n.data[2], n.data[3]]);
                        visit_nodes(&plugin.nodes, &mut |b, _| {
                            if b.header.form_id != base {
                                return;
                            }
                            let Ok(bs) = b.subrecords() else { return };
                            println!(
                                "      base {base:08X} {} {} SCRI={:?}",
                                String::from_utf8_lossy(&b.header.sig),
                                bs.iter()
                                    .find(|x| x.sig == *b"EDID")
                                    .map(|x| String::from_utf8_lossy(&x.data)
                                        .trim_end_matches('\0')
                                        .to_string())
                                    .unwrap_or_default(),
                                bs.iter().find(|x| x.sig == *b"SCRI").map(|x| u32::from_le_bytes([
                                    x.data[0], x.data[1], x.data[2], x.data[3]
                                ]))
                            );
                        });
                    }
                }
            }
        });
        if !found {
            println!("{label} {id:08X} <not in this plugin>");
        }
    };

    report(fid, "record");
}
