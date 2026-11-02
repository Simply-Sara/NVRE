//! Probe: the exact argument encodings of MoveTo, PMS, DamageAV,
//! AddItemHealthPercent in the corpus — arg count, tags, and the source
//! line that produced them.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_moveto
//! ```
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;

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
    let mut shown = 0usize;
    let targets: &[u16] = &[0x1123, 0x1181, 0x11BC, 0x1238];
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" || shown >= 40 {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
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
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        if compiled.is_empty() {
            return;
        }
        let ins = read_instructions(&compiled);
        let mut k = 0usize;
        while k < ins.len() && shown < 40 {
            let dotted = ins[k].opcode == 0x001C;
            let call = if dotted {
                if k + 1 >= ins.len() {
                    break;
                }
                &ins[k + 1]
            } else {
                &ins[k]
            };
            if targets.contains(&call.opcode) && call.args.len() >= 2 {
                let count = u16::from_le_bytes([call.args[0], call.args[1]]) as usize;
                let rest = &call.args[2..];
                // find the source line naming the command
                let cmdline = source.lines().find(|l| {
                    let u = l.to_ascii_uppercase();
                    u.contains("MOVETO")
                        || u.contains("PMS ")
                        || u.contains("DAMAGEAV")
                        || u.contains("DAMAGEACTORVALUE")
                        || u.contains("ADDITEMHEALTHPERCENT")
                        || u.contains("SETREPUTATION")
                });
                println!(
                    "{edid} op 0x{:04X} count {} args {:02X?}",
                    call.opcode, count, rest
                );
                if let Some(l) = cmdline {
                    println!("    src: {l}");
                }
                shown += 1;
            }
            k += if dotted { 2 } else { 1 };
        }
    });
}
