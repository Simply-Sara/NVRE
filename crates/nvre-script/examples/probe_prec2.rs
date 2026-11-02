//! Probe: for conditions mixing && and ||, print the source line AND the
//! game's full RPN operator sequence, so the precedence reading is settled
//! by eye on real units.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_prec2
//! ```
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
fn logical_ops(expr: &[u8]) -> Vec<&'static str> {
    let mut ops = Vec::new();
    let mut p = 0usize;
    while p < expr.len() {
        if expr[p] != 0x20 {
            p += 1;
            continue;
        }
        let start = p + 1;
        let mut end = start;
        while end < expr.len() && expr[end] != 0x20 {
            end += 1;
        }
        let tok = &expr[start..end];
        if tok == b"&&" {
            ops.push("&&");
        } else if tok == b"||" {
            ops.push("||");
        }
        p = if end > start { end } else { p + 1 };
    }
    ops
}
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
    let mut shown = 0;
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" || shown >= 12 {
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
        let ins = nvre_script::emit::read_instructions(&compiled);
        for x in &ins {
            if x.opcode != 0x16 && x.opcode != 0x18 {
                continue;
            }
            if x.args.len() < 4 {
                continue;
            }
            let expr_len = u16::from_le_bytes([x.args[2], x.args[3]]) as usize;
            let expr = &x.args[4..(4 + expr_len).min(x.args.len())];
            let ops = logical_ops(expr);
            if ops.contains(&"&&") && ops.contains(&"||") {
                // the source line: the first line containing if/elseif
                let line = source
                    .lines()
                    .find(|l| {
                        let u = l.to_ascii_lowercase();
                        (u.contains("if ") || u.contains("elseif"))
                            && (u.contains("&&") || u.contains("||"))
                    })
                    .map(|l| l.trim().to_string())
                    .unwrap_or_default();
                println!("{edid}: RPN ops {ops:?}");
                println!("    src: {line}");
                shown += 1;
                break;
            }
        }
    });
}
