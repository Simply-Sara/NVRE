//! Probe: the six jumpOps branches that don't fit `raw - 1C`, plus a census
//! of whether any if-branch body contains a nested Begin..End block at all.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_jumpops_edge
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;

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

/// The opcode's short name, for the disassembly listing.
fn name(op: u16) -> String {
    match op {
        0x10 => "Begin".into(),
        0x11 => "End".into(),
        0x15 => "Set".into(),
        0x16 => "If".into(),
        0x17 => "Else".into(),
        0x18 => "ElseIf".into(),
        0x19 => "EndIf".into(),
        0x1C => "SetCurrentRef".into(),
        0x1D => "ScriptName".into(),
        0x1E => "Return".into(),
        0x58 => "ExprCall".into(),
        other if other >= 0x1000 => format!("cmd 0x{other:04X}"),
        other => format!("op 0x{other:04X}"),
    }
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

    let wanted = [
        "VMcarranCurtisWarpTriggerSCRIPT",
        "CrNightkinCloakUntilFireScript",
        "VNovacChupacabraNightkinSCRIPT",
        "REPCONDavisonSCRIPT",
        "REPCONSuperMutantNightkinSCRIPT",
        "CrNightkinSCRIPT",
    ];

    // Census: how many branch bodies contain a nested Begin..End block.
    let mut branches_with_blocks = 0usize;
    let mut branches_total = 0usize;

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

        let want = wanted.contains(&edid.as_str());
        let ins = read_instructions(&compiled);

        // Walk for branches, same target logic as probe_jumpops.
        fn walk(
            ins: &[nvre_script::emit::Instruction],
            lo: usize,
            hi: usize,
            edid: &str,
            want: bool,
            compiled: &[u8],
            source: &str,
            branches_with_blocks: &mut usize,
            branches_total: &mut usize,
        ) {
            let mut i = lo;
            while i < hi {
                let op = ins[i].opcode;
                if op == 0x16 || op == 0x18 || op == 0x17 {
                    let mut chain_depth = 0usize;
                    let mut j = i + 1;
                    let mut raw = 0usize;
                    let mut scr = 0usize;
                    let mut has_block = false;
                    let mut target = hi;
                    while j < hi {
                        match ins[j].opcode {
                            0x10 => {
                                has_block = true;
                                let e = find_block_end(ins, j).unwrap_or(j);
                                raw += e - j + 1;
                                j = e;
                            }
                            0x1C => {
                                scr += 1;
                                raw += 1;
                            }
                            0x16 => {
                                chain_depth += 1;
                                raw += 1;
                            }
                            0x18 => {
                                if chain_depth == 0 {
                                    target = j;
                                    break;
                                }
                                raw += 1;
                            }
                            0x17 => {
                                if chain_depth == 0 {
                                    target = j;
                                    break;
                                }
                                raw += 1;
                            }
                            0x19 => {
                                if chain_depth == 0 {
                                    target = j;
                                    break;
                                }
                                chain_depth -= 1;
                                raw += 1;
                            }
                            _ => raw += 1,
                        }
                        j += 1;
                    }
                    *branches_total += 1;
                    if has_block {
                        *branches_with_blocks += 1;
                    }
                    let jump_ops = u16::from_le_bytes([ins[i].args[0], ins[i].args[1]]);
                    let fit = jump_ops as i64 == raw as i64 - scr as i64;
                    if want && !fit {
                        println!("=== {edid} branch op 0x{op:02X} jumpOps {jump_ops} raw {raw} 1C {scr} ===");
                        // the source, with line numbers
                        for (n, line) in source.lines().enumerate() {
                            println!("  src {n:>3}| {line}");
                        }
                        // the instruction stream
                        println!("  --- instructions ---");
                        for (k, x) in ins.iter().enumerate() {
                            let mark = if k == i { ">>" } else { "  " };
                            println!(
                                "  {mark} [{k:>3}] @{:<5} {} args {:02X?}",
                                x.at,
                                name(x.opcode),
                                x.args
                            );
                        }
                        println!("  (compiled len {})", compiled.len());
                        println!();
                    }
                    if i + 1 < target {
                        walk(
                            ins,
                            i + 1,
                            target,
                            edid,
                            want,
                            compiled,
                            source,
                            branches_with_blocks,
                            branches_total,
                        );
                    }
                    i = if target < hi && ins[target].opcode != 0x19 {
                        target
                    } else {
                        target + 1
                    };
                } else if op == 0x10 {
                    let e = find_block_end(ins, i).unwrap_or(i);
                    walk(
                        ins,
                        i + 1,
                        e,
                        edid,
                        want,
                        compiled,
                        source,
                        branches_with_blocks,
                        branches_total,
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
            &edid,
            want,
            &compiled,
            &source,
            &mut branches_with_blocks,
            &mut branches_total,
        );
    });

    println!("census: {branches_total} branches, {branches_with_blocks} contain a nested Begin..End block");
}
