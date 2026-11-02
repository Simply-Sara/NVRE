//! Probe: a declared ref variable as a set VALUE — when does the game write
//! the variable form (66 <id>) and when the reference form (72 <idx>)?
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_setval
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
    // Find Set instructions (0x0015) whose value bytes start 72 (ref form)
    // and whose source line is `set X to <name>` where <name> is a declared
    // ref variable. Print the unit, the source line, the destination bytes
    // and the value bytes.
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
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
        // declared ref names
        let decl_refs: Vec<String> = source
            .lines()
            .filter_map(|l| {
                let t = l.trim();
                let low = t.to_ascii_lowercase();
                if low.starts_with("ref ") || low.starts_with("reference ") {
                    t.split_whitespace()
                        .nth(1)
                        .map(|n| n.trim_end_matches(';').to_string())
                } else {
                    None
                }
            })
            .collect();
        if decl_refs.is_empty() {
            return;
        }
        for ins in read_instructions(&compiled) {
            if ins.opcode != 0x0015 || ins.args.len() < 5 {
                continue;
            }
            // Set args: [dest tag+idx][len:u16][20 value...]
            let dest_tag = ins.args[0];
            // value starts after dest (3 bytes) + len (2 bytes)
            if ins.args.len() < 6 {
                continue;
            }
            let val_len = u16::from_le_bytes([ins.args[3], ins.args[4]]) as usize;
            let val = &ins.args[5..(5 + val_len).min(ins.args.len())];
            // a value that is exactly 72 idx (ref form, 3 bytes) — the game
            // writing a bare reference as the assigned value
            if val.len() == 3 && val[0] == 0x72 && dest_tag == 0x73 {
                // the source line: find `set <dest> to <refname>`
                for line in source.lines() {
                    let u = line.to_ascii_lowercase();
                    if u.starts_with("set ") && u.contains(" to ") {
                        // the value name: the word after 'to'
                        let after = u.split(" to ").nth(1).unwrap_or("");
                        let valname = after
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .trim_end_matches(';');
                        if decl_refs.iter().any(|r| r.eq_ignore_ascii_case(valname)) {
                            println!(
                                "{edid}: Set dest tag {dest_tag:02X} args {:02X?} | src: {line}",
                                ins.args
                            );
                        }
                    }
                }
            }
        }
    });
}
