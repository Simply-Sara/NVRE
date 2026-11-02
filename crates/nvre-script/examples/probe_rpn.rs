//! Probe: decode a unit's If expression tokens (0x20-separated) with the
//! game's own bytes, so an RPN shape can be read rather than guessed.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_rpn -- <hex-formid>
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
    let wanted = u32::from_str_radix(&std::env::args().nth(1).unwrap_or_default(), 16).unwrap_or(0);
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.form_id != wanted || rec.header.sig != *b"SCPT" {
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
        println!("=== {edid} ===");
        for (n, l) in source.lines().enumerate() {
            if l.contains("if ") || l.contains("If ") || l.contains("elseif") {
                println!("src{n}| {l}");
            }
        }
        // find If instructions and decode their expressions
        let mut i = 0;
        while i + 4 <= compiled.len() {
            let op = u16::from_le_bytes([compiled[i], compiled[i + 1]]);
            if op == 0x001C {
                i += 4;
                continue;
            }
            let len = u16::from_le_bytes([compiled[i + 2], compiled[i + 3]]) as usize;
            let args = &compiled[i + 4..(i + 4 + len).min(compiled.len())];
            if (op == 0x16 || op == 0x18) && args.len() >= 4 {
                let expr_len = u16::from_le_bytes([args[2], args[3]]) as usize;
                let expr = &args[4..(4 + expr_len).min(args.len())];
                println!("If @{} exprLen {expr_len}:", i);
                // tokens separated by 0x20
                let mut p = 0;
                let mut tok = 0;
                while p < expr.len() {
                    if expr[p] == 0x20 {
                        // read the token after the separator until the next separator
                        let start = p + 1;
                        let mut end = start;
                        while end < expr.len() && expr[end] != 0x20 {
                            end += 1;
                        }
                        let t = &expr[start..end];
                        let mut desc = format!("{:02X?}", t);
                        if t.len() == 3
                            && (t[0] == 0x73
                                || t[0] == 0x66
                                || t[0] == 0x72
                                || t[0] == 0x5A
                                || t[0] == 0x47)
                        {
                            let idx = u16::from_le_bytes([t[1], t[2]]);
                            desc = format!("tag {:02X} idx {idx}", t[0]);
                        } else if t.len() == 7 && t[0] == 0x58 {
                            let opcode = u16::from_le_bytes([t[1], t[2]]);
                            let pb = u16::from_le_bytes([t[3], t[4]]);
                            desc = format!(
                                "X call 0x{opcode:04X} paramBytes {pb} +{} more",
                                t.len() - 5
                            );
                        } else if t.len() == 2
                            && (t[0] == b'='
                                || t[0] == b'!'
                                || t[0] == b'&'
                                || t[0] == b'|'
                                || t[0] == b'<'
                                || t[0] == b'>'
                                || t[0] == b'+'
                                || t[0] == b'-'
                                || t[0] == b'*'
                                || t[0] == b'/'
                                || t[0] == b'%'
                                || t[0] == b'~')
                        {
                            desc = format!("op '{}'", String::from_utf8_lossy(t));
                        } else if t.iter().all(|b| b.is_ascii_digit()) {
                            desc = format!("literal {}", String::from_utf8_lossy(t));
                        }
                        println!("  tok{tok}: {desc}");
                        tok += 1;
                        p = end;
                    } else {
                        p += 1;
                    }
                }
            }
            i += 4 + len;
        }
    });
}
