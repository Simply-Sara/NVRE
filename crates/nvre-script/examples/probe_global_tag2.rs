//! Probe: does the SCRO entry's record type decide `47` (Global) vs `5A`
//! (ref)? For every `20 47 <idx>` and `20 5A <idx>` operand in the corpus,
//! resolve the idx through the unit's ref space to the SCRO FormID, then to
//! that record's signature, and tabulate tag by signature.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_global_tag2
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

    // FormID -> record signature, for every record in the master.
    let mut sig_of: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        sig_of.insert(
            rec.header.form_id,
            String::from_utf8_lossy(&rec.header.sig).into_owned(),
        );
    });

    // tag -> record signature of the named SCRO entry
    let mut tag_by_sig: std::collections::BTreeMap<u8, std::collections::BTreeMap<String, usize>> =
        std::collections::BTreeMap::new();
    let mut unresolved = 0usize;

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
        if compiled.is_empty() || source.is_empty() {
            return;
        }
        let mut decl_refs = 0usize;
        for line in source.lines() {
            let t = line.trim();
            let low = t.to_ascii_lowercase();
            if low.starts_with("ref ") || low.starts_with("reference ") {
                decl_refs += 1;
            }
        }
        let scro_fids: Vec<u32> = subs
            .iter()
            .filter(|x| x.sig == *b"SCRO" && x.data.len() == 4)
            .map(|x| u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]))
            .collect();

        let ins = nvre_script::emit::read_instructions(&compiled);
        fn walk(
            ins: &[nvre_script::emit::Instruction],
            lo: usize,
            hi: usize,
            decl_refs: usize,
            scro_fids: &[u32],
            sig_of: &std::collections::HashMap<u32, String>,
            tag_by_sig: &mut std::collections::BTreeMap<
                u8,
                std::collections::BTreeMap<String, usize>,
            >,
            unresolved: &mut usize,
        ) {
            let mut i = lo;
            while i < hi {
                let op = ins[i].opcode;
                if op == 0x16 || op == 0x18 {
                    let args = &ins[i].args;
                    if args.len() >= 4 {
                        let expr_len = u16::from_le_bytes([args[2], args[3]]) as usize;
                        let expr = &args[4..(4 + expr_len).min(args.len())];
                        let mut p = 0usize;
                        while p + 1 < expr.len() {
                            if expr[p] != 0x20 {
                                p += 1;
                                continue;
                            }
                            let tag = expr[p + 1];
                            if (tag == 0x47 || tag == 0x5A) && p + 4 <= expr.len() {
                                let idx = u16::from_le_bytes([expr[p + 2], expr[p + 3]]) as usize;
                                if idx > decl_refs && idx - decl_refs - 1 < scro_fids.len() {
                                    let fid = scro_fids[idx - decl_refs - 1];
                                    let sig = sig_of
                                        .get(&fid)
                                        .cloned()
                                        .unwrap_or_else(|| "__unknown__".into());
                                    *tag_by_sig.entry(tag).or_default().entry(sig).or_insert(0) +=
                                        1;
                                } else {
                                    *unresolved += 1;
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
                    i += 1;
                } else if op == 0x10 {
                    let e = find_block_end(ins, i).unwrap_or(i);
                    walk(
                        ins,
                        i + 1,
                        e,
                        decl_refs,
                        scro_fids,
                        sig_of,
                        tag_by_sig,
                        unresolved,
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
            decl_refs,
            &scro_fids,
            &sig_of,
            &mut tag_by_sig,
            &mut unresolved,
        );
    });

    println!("tag -> SCRO entry's record signature:");
    for (tag, m) in &tag_by_sig {
        let total: usize = m.values().sum();
        println!("  tag 0x{tag:02X} ({total}):");
        let mut rows: Vec<(&String, &usize)> = m.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1));
        for (sig, n) in rows.iter().take(8) {
            println!("    {sig:<8} {n}");
        }
    }
    println!("unresolved indices: {unresolved}");
}
