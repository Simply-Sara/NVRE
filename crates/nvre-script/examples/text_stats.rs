//! Probe: how many scripts ship without their source.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example text_stats
//! ```
//!
//! A `SCPT` carries `SCTX` (the source the GECK kept) and `SCDA` (the compiled
//! bytes). The editor shows `SCTX`, so a script whose source was stripped — the
//! GECK's "save script source" option, and every mod that ticked it off — is
//! unreadable in the editor even though the game runs it. Those are the records
//! a decompiler would be for, so this counts them first.
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA");
        return;
    };
    let dir = std::path::Path::new(&data);
    let mut names: Vec<String> = std::env::args().skip(1).collect();
    if names.is_empty() {
        names = vec!["FalloutNV.esm".to_string()];
    }
    for name in names {
        let path = dir.join(&name);
        let Ok(bytes) = std::fs::read(&path) else {
            println!("{name:<28} <not found>");
            continue;
        };
        let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), name.clone()) else {
            println!("{name:<28} <cannot parse>");
            continue;
        };
        let mut total = 0usize;
        let mut with_source = 0usize;
        let mut no_source = 0usize;
        let mut no_compiled = 0usize;
        let mut both = 0usize;
        let mut examples: Vec<String> = Vec::new();
        visit_nodes(&plugin.nodes, &mut |rec, _| {
            if rec.header.sig != *b"SCPT" {
                return;
            }
            total += 1;
            let Ok(subs) = rec.subrecords() else { return };
            let edid = subs
                .iter()
                .find(|x| x.sig == *b"EDID")
                .map(|x| String::from_utf8_lossy(&x.data).trim_end_matches('\0').to_string())
                .unwrap_or_default();
            let source = subs
                .iter()
                .find(|x| x.sig == *b"SCTX")
                .map(|x| x.data.len() > 1)
                .unwrap_or(false);
            let compiled = subs
                .iter()
                .find(|x| x.sig == *b"SCDA")
                .map(|x| !x.data.is_empty())
                .unwrap_or(false);
            match (source, compiled) {
                (true, true) => both += 1,
                (true, false) => with_source += 1,
                (false, true) => {
                    no_source += 1;
                    if examples.len() < 8 {
                        examples.push(edid);
                    }
                }
                (false, false) => no_compiled += 1,
            }
        });
        println!(
            "{name:<28} SCPT {total:>6}  source+bytes {both:>6}  source only {with_source:>4}  bytes only {no_source:>5}  neither {no_compiled:>4}"
        );
        if !examples.is_empty() {
            println!("      bytes only, e.g. {}", examples.join(", "));
        }
    }
}
