//! Probe: what does `jumpOps` actually count?
//!
//! The emitter writes the count of instructions in the branch body, but the
//! corpus's top failure bucket is `If`/`ElseIf`/`Else` with the skip count
//! wrong, systematically over-counted. This probe reads the game's own bytes
//! and tabulates candidate counting rules, so the rule is derived rather
//! than guessed.
//!
//! For each branch, the jump target is the next branch keyword at the same
//! lexical depth: `ElseIf`/`Else`/`EndIf` for an `If` or `ElseIf`, and
//! `EndIf` for an `Else`. Between the branch header and its target, the
//! candidate rules disagree about how nested `Begin..End` blocks and
//! `0x1C SetCurrentRef`+command pairs count.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_jumpops
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;

/// Find the End that closes the Begin at `begin`.
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

/// One branch's measurement: the raw instruction count between its header and
/// its jump target, plus the ingredients every candidate rule is built from.
struct IfInfo {
    op: u16,
    jump_ops: u16,
    /// instructions strictly between this branch's header and its target
    raw: usize,
    /// complete `Begin..End` blocks in that span
    blocks: usize,
    /// total instructions those blocks occupy, framing included
    block_ins: usize,
    /// `0x1C` SetCurrentRef prefixes in that span
    scr: usize,
}

/// Measure every branch in `ins[lo..hi]`, recursing into nested blocks.
fn scan(ins: &[nvre_script::emit::Instruction], lo: usize, hi: usize, out: &mut Vec<IfInfo>) {
    let mut i = lo;
    while i < hi {
        let op = ins[i].opcode;
        if op == 0x16 || op == 0x18 || op == 0x17 {
            // Walk to the jump target: the next branch keyword at the same
            // chain depth. A nested `if` in the body opens its own chain —
            // its elseif/else/endif belong to it, not to this one — so the
            // chain depth tracks that: a nested If raises it, its EndIf
            // lowers it, and its ElseIf/Else are just body instructions.
            let mut chain_depth = 0usize;
            let mut j = i + 1;
            let mut raw = 0usize;
            let mut blocks = 0usize;
            let mut block_ins = 0usize;
            let mut scr = 0usize;
            let mut target = hi;
            while j < hi {
                match ins[j].opcode {
                    0x10 => {
                        blocks += 1;
                        let e = find_block_end(ins, j).unwrap_or(j);
                        let span = e - j + 1;
                        raw += span;
                        block_ins += span;
                        j = e;
                    }
                    0x1C => {
                        scr += 1;
                        raw += 1;
                    }
                    0x16 => {
                        // A nested if: its whole chain is body.
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
            out.push(IfInfo {
                op,
                jump_ops: u16::from_le_bytes([ins[i].args[0], ins[i].args[1]]),
                raw,
                blocks,
                block_ins,
                scr,
            });
            // The target is the next branch's own header when it is one, so
            // the walk resumes *at* it to measure that branch too. The body
            // between the header and the target is scanned as well: nested
            // chains live there, and their branches count too.
            if i + 1 < target {
                scan(ins, i + 1, target, out);
            }
            i = if target < hi && ins[target].opcode != 0x19 {
                target
            } else {
                target + 1
            };
        } else if op == 0x10 {
            let e = find_block_end(ins, i).unwrap_or(i);
            scan(ins, i + 1, e, out);
            i = e + 1;
        } else {
            i += 1;
        }
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

    struct Unit {
        edid: String,
        compiled: Vec<u8>,
    }
    let mut units: Vec<Unit> = Vec::new();
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
        if !compiled.is_empty() {
            units.push(Unit { edid, compiled });
        }
    });

    let mut total = 0usize;
    // candidate rules over (raw, blocks, block_ins, scr):
    let mut a_ok = 0usize; // raw
    let mut b_ok = 0usize; // raw - blocks
    let mut c_ok = 0usize; // raw - blocks - scr
    let mut e_ok = 0usize; // raw - scr
    let mut f_ok = 0usize; // raw - block_ins + blocks - scr (a block is 1 op, a 1C pair is 1 op)
    let mut g_ok = 0usize; // raw - block_ins + blocks
    let mut mismatches: Vec<String> = Vec::new();

    for u in &units {
        let ins = read_instructions(&u.compiled);
        let mut infos = Vec::new();
        scan(&ins, 0, ins.len(), &mut infos);
        for f in &infos {
            total += 1;
            let raw = f.raw as i64;
            let nb = f.blocks as i64;
            let nbi = f.block_ins as i64;
            let ns = f.scr as i64;
            let j = f.jump_ops as i64;
            let cands = [
                ("A", raw),
                ("B", raw - nb),
                ("C", raw - nb - ns),
                ("E", raw - ns),
                ("F", raw - nbi + nb - ns),
                ("G", raw - nbi + nb),
            ];
            for (name, v) in cands {
                let ok = j == v;
                match name {
                    "A" => a_ok += ok as usize,
                    "B" => b_ok += ok as usize,
                    "C" => c_ok += ok as usize,
                    "E" => e_ok += ok as usize,
                    "F" => f_ok += ok as usize,
                    "G" => g_ok += ok as usize,
                    _ => {}
                }
            }
            if j != raw - nbi + nb - ns && mismatches.len() < 20 {
                mismatches.push(format!(
                    "{:<30} op 0x{:02X} jumpOps {:>4}  raw {:>4}  blockIns {:>4}  blocks {}  1C {}",
                    u.edid, f.op, j, raw, nbi, nb, ns
                ));
            }
        }
    }

    println!("branches scanned: {total}");
    println!(
        "A  raw:                                   {a_ok:>5}  ({}%)",
        a_ok * 100 / total.max(1)
    );
    println!(
        "B  raw - blocks:                          {b_ok:>5}  ({}%)",
        b_ok * 100 / total.max(1)
    );
    println!(
        "C  raw - blocks - 1C:                     {c_ok:>5}  ({}%)",
        c_ok * 100 / total.max(1)
    );
    println!(
        "E  raw - 1C:                              {e_ok:>5}  ({}%)",
        e_ok * 100 / total.max(1)
    );
    println!(
        "F  raw - blockIns + blocks - 1C:          {f_ok:>5}  ({}%)",
        f_ok * 100 / total.max(1)
    );
    println!(
        "G  raw - blockIns + blocks:               {g_ok:>5}  ({}%)",
        g_ok * 100 / total.max(1)
    );
    println!();
    println!("mismatches against F:");
    for m in &mismatches {
        println!("  {m}");
    }
}
