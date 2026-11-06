//! Probe: blockLen vs the actual block body, corpus-wide, for a script's
//! FIRST Begin (the common single-block shape).
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_blocklen
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
    // blockLen - (body through the matching End) for the first Begin of
    // every unit, tabulated.
    let mut diffs: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
    let mut examples: Vec<String> = Vec::new();
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
        // find the first Begin
        let Some(begin) = ins.iter().find(|x| x.opcode == 0x0010) else {
            return;
        };
        if begin.args.len() < 6 {
            return;
        }
        let block_len =
            u32::from_le_bytes([begin.args[2], begin.args[3], begin.args[4], begin.args[5]]) as i64;
        // the block body: from after the Begin to its matching End.
        // Walk instructions from begin+1 with a depth counter.
        let bi = ins
            .iter()
            .position(|x| x.opcode == 0x0010 && x.at == begin.at)
            .unwrap();
        let mut depth = 0i64;
        let mut body_end = None;
        for x in &ins[bi + 1..] {
            if x.opcode == 0x0010 {
                depth += 1;
            }
            if x.opcode == 0x0011 {
                if depth == 0 {
                    body_end = Some(x.at + 4);
                    break;
                }
                depth -= 1;
            }
        }
        let Some(end) = body_end else { return };
        let body = (end - (begin.at + 4 + begin.args.len())) as i64;
        let d = block_len - body;
        *diffs.entry(d).or_insert(0) += 1;
        if d != 0 && examples.len() < 10 {
            examples.push(format!("{edid}: blockLen {block_len} body {body} diff {d}"));
        }
    });
    for (d, n) in diffs.iter().rev().take(12) {
        println!("blockLen - body = {d:>5}: {n} units");
    }
    for e in &examples {
        println!("  {e}");
    }
}
