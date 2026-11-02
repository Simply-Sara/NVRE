//! Probe: when does a standalone reference in an expression write `47`
//! (Global) vs `5A` (ref)? Correlate the game's tag with the SCRO entry's
//! record type.
//!
//! The skill documents `47` 'G' + refIdx for globals (GameHour etc.), but
//! the emitter special-cases six names instead of measuring the rule. This
//! probe walks every If/ElseIf expression, finds `20 47 <idx>` and `20 5A
//! <idx>` operands, resolves the idx through the unit's declared-refs-then-
//! SCRO space, and prints the SCRO entry's record signature for each.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_global_tag
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

fn find_block_end(ins: &[nvre_script::emit::Instruction], begin: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, x) in ins.iter().enumerate().skip(begin + 1) {
        match x.opcode {
            0x10 => depth += 1,
            0x11 => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

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

    // tag -> the record signatures of the SCRO entries it named, corpus-wide
    let mut tag_sigs: std::collections::BTreeMap<u8, std::collections::BTreeMap<String, usize>> =
        std::collections::BTreeMap::new();
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
        if compiled.is_empty() || source.is_empty() {
            return;
        }
        // This record's reference space: declared ref names (source order),
        // then SCRO FormIDs resolved to their records' signatures.
        let mut decl_refs: Vec<String> = Vec::new();
        for line in source.lines() {
            let t = line.trim();
            let low = t.to_ascii_lowercase();
            if low.starts_with("ref ") || low.starts_with("reference ") {
                let rest = t.split_whitespace().nth(1).unwrap_or("");
                decl_refs.push(rest.trim_end_matches(';').to_string());
            }
        }
        let scro_fids: Vec<u32> = subs
            .iter()
            .filter(|x| x.sig == *b"SCRO" && x.data.len() == 4)
            .map(|x| u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]))
            .collect();

        // Walk the If/ElseIf expressions for 20 47/5A/72 idx.
        let ins = nvre_script::emit::read_instructions(&compiled);
        fn walk(
            ins: &[nvre_script::emit::Instruction],
            lo: usize,
            hi: usize,
            decl_refs: &[String],
            scro_fids: &[u32],
            tag_sigs: &mut std::collections::BTreeMap<
                u8,
                std::collections::BTreeMap<String, usize>,
            >,
            examples: &mut Vec<String>,
            edid: &str,
        ) {
            let mut i = lo;
            while i < hi {
                let op = ins[i].opcode;
                if op == 0x16 || op == 0x18 {
                    let args = &ins[i].args;
                    if args.len() >= 4 {
                        let expr_len = u16::from_le_bytes([args[2], args[3]]) as usize;
                        let expr = &args[4..(4 + expr_len).min(args.len())];
                        // tokens: 20 tag payload
                        let mut p = 0usize;
                        while p + 1 < expr.len() {
                            if expr[p] != 0x20 {
                                p += 1;
                                continue;
                            }
                            let tag = expr[p + 1];
                            if (tag == 0x47 || tag == 0x5A || tag == 0x72) && p + 4 <= expr.len() {
                                let idx = u16::from_le_bytes([expr[p + 2], expr[p + 3]]) as usize;
                                // resolve idx through the ref space
                                let sig = if idx >= 1 && idx <= decl_refs.len() {
                                    "declared-ref".to_string()
                                } else if idx > decl_refs.len()
                                    && idx - decl_refs.len() - 1 < scro_fids.len()
                                {
                                    // the SCRO entry's record signature —
                                    // resolved by the caller below via the
                                    // fid->sig map; here mark it
                                    format!("scro@{}", idx - decl_refs.len() - 1)
                                } else {
                                    "out-of-range".to_string()
                                };
                                let _ = sig;
                                *tag_sigs
                                    .entry(tag)
                                    .or_default()
                                    .entry(sig.clone())
                                    .or_insert(0) += 1;
                                if tag == 0x47 && examples.len() < 20 {
                                    examples.push(format!(
                                        "{edid}: 47 idx {idx} (decl {})",
                                        decl_refs.len()
                                    ));
                                }
                                p += 4;
                                continue;
                            }
                            if tag == 0x66 || tag == 0x73 {
                                p += 4;
                                continue;
                            }
                            if tag == 0x58 {
                                p += 2;
                                continue;
                            }
                            p += 1;
                        }
                    }
                    // skip to EndIf at same depth (approx: next 19/17/16/18 at depth 0) — not needed for the tag census
                    i += 1;
                } else if op == 0x10 {
                    let e = find_block_end(ins, i).unwrap_or(i);
                    walk(
                        ins,
                        i + 1,
                        e,
                        decl_refs,
                        scro_fids,
                        tag_sigs,
                        examples,
                        edid,
                    );
                    i = e + 1;
                } else {
                    i += 1;
                }
            }
        }
        walk(
            &ins,
            0,
            ins.len(),
            &decl_refs,
            &scro_fids,
            &mut tag_sigs,
            &mut examples,
            &edid,
        );
    });

    println!("tag -> what its indices resolved to (declared-ref vs scro position):");
    for (tag, m) in &tag_sigs {
        let total: usize = m.values().sum();
        let declared = m.get("declared-ref").copied().unwrap_or(0);
        let scro = total - declared;
        println!(
            "  tag {:02X}: {} total ({} declared-ref, {} scro)",
            tag, total, declared, scro
        );
    }
    println!();
    println!("47 examples (which index space do they name?):");
    for e in &examples {
        println!("  {e}");
    }
}
