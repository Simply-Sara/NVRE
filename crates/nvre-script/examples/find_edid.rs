//! Probe: every record whose `EDID` contains a substring, with its FormID and
//! signature — the question "which form does this name mean?".
//!
//! ```text
//! NVRE_DATA=<Data> NVRE_PLUGIN=DeadMoney.esm \
//!   cargo run -p nvre-script --example find_edid -- HologramStopPackage
//! ```
//!
//! Written to settle the three residual units whose source spells a name that
//! is not in the record's reference list: `HologramStopPackage`,
//! `NVDLC02SorrowMQ05REF`, `NVDLC03SinkNukaColaMachineMsg`. Three units is not
//! enough evidence for a resolution rule, so the first question is whether the
//! name the source spells exists as a form at all, and in which plugin.
//!
//! Every plugin is searched: the name may live in a master rather than in the
//! file being compiled, and a hit in the master with a same-shaped name in the
//! DLC is a different answer from a hit nowhere.
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA to the game's Data folder");
        return;
    };
    let Some(needle) = std::env::args().nth(1) else {
        eprintln!("usage: find_edid <substring>");
        return;
    };
    let needle = needle.to_ascii_lowercase();

    let mut plugins: Vec<String> = vec![
        std::env::var("NVRE_PLUGIN").unwrap_or_else(|_| "FalloutNV.esm".to_string()),
    ];
    // The plugin's own masters, so a name that lives in the master is not
    // reported as absent.
    if let Ok(bytes) = std::fs::read(std::path::Path::new(&data).join(&plugins[0])) {
        if let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), plugins[0].clone()) {
            let mut names: Vec<String> = Vec::new();
            visit_nodes(&plugin.nodes, &mut |rec, _| {
                if rec.header.sig != *b"TES4" {
                    return;
                }
                let Ok(subs) = rec.subrecords() else { return };
                for s in subs.iter() {
                    if s.sig == *b"MAST" {
                        names.push(
                            String::from_utf8_lossy(&s.data)
                                .trim_end_matches('\0')
                                .to_string(),
                        );
                    }
                }
            });
            for n in names {
                if !plugins.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
                    plugins.push(n);
                }
            }
        }
    }

    let mut total = 0usize;
    for name in &plugins {
        let path = std::path::Path::new(&data).join(name);
        let Ok(bytes) = std::fs::read(&path) else {
            println!("{name}: not present");
            continue;
        };
        let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), name.clone()) else {
            println!("{name}: did not parse");
            continue;
        };
        let mut hits = 0usize;
        visit_nodes(&plugin.nodes, &mut |rec, _| {
            let Ok(subs) = rec.subrecords() else { return };
            let Some(e) = subs.iter().find(|x| x.sig == *b"EDID") else {
                return;
            };
            let edid = String::from_utf8_lossy(&e.data)
                .trim_end_matches('\0')
                .to_string();
            if !edid.to_ascii_lowercase().contains(&needle) {
                return;
            }
            hits += 1;
            println!(
                "{name}  {:08X}  {}  {edid}",
                rec.header.form_id,
                String::from_utf8_lossy(&rec.header.sig)
            );
        });
        total += hits;
    }
    println!("\n{total} record(s) named like {needle:?}");
}
