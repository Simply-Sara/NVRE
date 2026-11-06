//! Probe: measure each reference-index space separately, pairing source
//! names with bytecode indices by order of use.
//!
//! The corpus shows the index a reference carries is not one space: a
//! dotted-expression `72` prefix, a `5A`/`Z` standalone operand, a `1C`
//! statement prefix, a command argument, and a `Begin` parameter can each
//! count differently. This probe walks each unit's source and bytecode
//! together, extracts every reference index the game wrote per space, and
//! prints the unit's declared refs and SCRO list beside them — so each
//! space's rule can be read off, one space at a time.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_ref_spaces
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA");
        return;
    };
    let path = std::path::Path::new(&data).join("FalloutNV.esm");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("cannot read master");
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), "FalloutNV.esm".to_string()) else {
        eprintln!("cannot parse master");
        return;
    };

    // For a handful of units, print: declared refs (source order), SCRO
    // names, and every ref-carrying index in the bytecode with its offset,
    // grouped by the shape that carries it.
    let wanted: &[&str] = &[
        "NVEcvDoorBG01SCRIPT",
        "vCCCrucifiedSlaveSCRIPT",
        "VFSOrrisThugSleepScript",
    ];

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
        if !wanted.contains(&edid.as_str()) {
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
        if compiled.is_empty() {
            return;
        }
        let scro: Vec<String> = subs
            .iter()
            .filter(|x| x.sig == *b"SCRO" && x.data.len() == 4)
            .map(|x| {
                let fid = u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]);
                format!("{fid:08X}")
            })
            .collect();
        let decl_refs: Vec<String> = source
            .lines()
            .filter_map(|line| {
                let t = line.trim();
                let low = t.to_ascii_lowercase();
                let rest = if low.starts_with("ref ") {
                    t.split_once([' ', '\t']).map(|(_, r)| r)
                } else if low.starts_with("reference ") {
                    t.split_once([' ', '\t']).map(|(_, r)| r)
                } else {
                    None
                }?;
                let name = rest.split_whitespace().next()?;
                Some(name.trim_end_matches(';').to_string())
            })
            .collect();

        println!("=== {edid} ===");
        println!("declared refs ({}): {decl_refs:?}", decl_refs.len());
        println!("SCRO ({}): {scro:?}", scro.len());

        // Walk instructions structurally.
        let mut i = 0usize;
        let mut ins_no = 0usize;
        while i + 4 <= compiled.len() {
            let op = u16::from_le_bytes([compiled[i], compiled[i + 1]]);
            if op == 0x001C {
                let idx = u16::from_le_bytes([compiled[i + 2], compiled[i + 3]]);
                println!("  @{i:<5} 1C-stmt-prefix      refIdx {idx}");
                i += 4;
                continue;
            }
            let len = u16::from_le_bytes([compiled[i + 2], compiled[i + 3]]) as usize;
            let args = &compiled[i + 4..(i + 4 + len).min(compiled.len())];
            // Begin params: event + blockLen + count + params
            if op == 0x0010 && args.len() >= 8 {
                let count = u16::from_le_bytes([args[6], args[7]]) as usize;
                let mut p = 8usize;
                for _ in 0..count {
                    if p + 3 > args.len() {
                        break;
                    }
                    if args[p] == 0x72 {
                        let idx = u16::from_le_bytes([args[p + 1], args[p + 2]]);
                        println!("  @{i:<5} Begin-param          refIdx {idx}");
                    }
                    p += 3;
                }
            }
            // Expression dotted prefix: 72 idx 58 inside any args.
            for (w_at, w) in args.windows(4).enumerate() {
                if w[0] == 0x72 && w[3] == 0x58 {
                    let idx = u16::from_le_bytes([w[1], w[2]]);
                    println!("  @{i:<5} 72-expr-dotted(+{})   refIdx {idx}", w_at);
                }
            }
            // Standalone operand tags inside expressions: 5A/47 followed by
            // u16 — only inside If/ElseIf (op 16/18) args, after a 0x20.
            if (op == 0x0016 || op == 0x0018) && args.len() >= 4 {
                let expr_len = u16::from_le_bytes([args[2], args[3]]) as usize;
                let expr = &args[4..(4 + expr_len).min(args.len())];
                let mut p = 0usize;
                while p + 3 <= expr.len() {
                    if expr[p] == 0x20 {
                        let tag = expr[p + 1];
                        if tag == 0x5A || tag == 0x47 {
                            let idx = u16::from_le_bytes([expr[p + 2], expr[p + 3]]);
                            println!("  @{i:<5} expr-standalone {:02X}     refIdx {idx}", tag);
                            p += 4;
                            continue;
                        }
                        if tag == 0x72 {
                            let idx = u16::from_le_bytes([expr[p + 2], expr[p + 3]]);
                            println!("  @{i:<5} expr-operand 72       refIdx {idx}",);
                            p += 4;
                            continue;
                        }
                    }
                    p += 1;
                }
            }
            // Command arguments: count then tagged values.
            if op >= 0x1000 && args.len() >= 2 {
                let count = u16::from_le_bytes([args[0], args[1]]) as usize;
                let mut p = 2usize;
                for _ in 0..count {
                    if p >= args.len() {
                        break;
                    }
                    match args[p] {
                        0x72 | 0x5A => {
                            if p + 3 <= args.len() {
                                let idx = u16::from_le_bytes([args[p + 1], args[p + 2]]);
                                println!(
                                    "  @{i:<5} cmd-arg {:02X}             refIdx {idx}",
                                    args[p]
                                );
                            }
                            p += 3;
                        }
                        0x6E => p += 5,
                        0x7A => p += 9,
                        0x73 | 0x66 | 0x47 => p += 3,
                        _ => break,
                    }
                }
            }
            ins_no += 1;
            i += 4 + len;
        }
        let _ = ins_no;
        println!();
    });
}
