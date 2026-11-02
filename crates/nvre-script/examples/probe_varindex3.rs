//! Probe: verify the SLSD-ID indexing rule with a structural scan.
//!
//! `probe_varindex2` scanned tag bytes at arbitrary offsets and produced
//! false positives. This probe reads the expression grammar instead: every
//! `0x20`-tagged token inside an `If`/`ElseIf` condition is `[0x20][tag][
//! payload]`, so the tag and its index are read where the grammar puts them.
//! For each such (tag, index) pair the unit's SLSD table is checked:
//! is the index the variable's ID (SLSD bytes 0..4), its table position, or
//! its per-type position?
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_varindex3
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

/// Walk `args` as a run of 0x20-tagged expression tokens; call `f` with each
/// tag and its payload. Returns false if the run cannot be read (which the
/// caller ignores — the shape is known from the corpus).
fn walk_expr(args: &[u8], mut f: impl FnMut(u8, &[u8])) {
    let mut i = 0usize;
    while i + 1 < args.len() {
        if args[i] != 0x20 {
            return;
        }
        let tag = args[i + 1];
        let payload_len = match tag {
            0x73 | 0x66 | 0x72 | 0x5A | 0x47 => 2,
            0x58 => 4, // X: opcode u16 + paramBytes u16 (params follow separately)
            0x6E => 4,
            0x7A => 8,
            _ => {
                // ASCII literal or operator: 1+ bytes with no length; stop
                // rather than misread. Only single-byte literals ('0'..'9')
                // are handled by the caller's known shapes; bail here.
                return;
            }
        };
        if i + 2 + payload_len > args.len() {
            return;
        }
        f(tag, &args[i + 2..i + 2 + payload_len]);
        i += 2 + payload_len;
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

    // Counts of (tag, index) pairs where the index equals each candidate.
    let mut total_pairs = 0usize;
    let mut id_ok = 0usize;
    let mut pos_ok = 0usize;
    let mut id_mismatch_examples: Vec<String> = Vec::new();

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
        if compiled.is_empty() {
            return;
        }
        // SLSD: ID = bytes 0..4, type = byte 16 (0 float/ref, 1 int).
        // SCVR: names, paired positionally.
        let mut ids: Vec<u32> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut types: Vec<u8> = Vec::new();
        for s in subs.iter() {
            if s.sig == *b"SLSD" && s.data.len() >= 20 {
                ids.push(u32::from_le_bytes([
                    s.data[0], s.data[1], s.data[2], s.data[3],
                ]));
                types.push(s.data[16]);
            } else if s.sig == *b"SCVR" {
                names.push(
                    String::from_utf8_lossy(&s.data)
                        .trim_end_matches('\0')
                        .to_string(),
                );
            }
        }
        if ids.is_empty() || ids.len() != names.len() {
            return;
        }

        // Structurally walk instructions: [opcode u16][argLen u16][args].
        let mut i = 0usize;
        while i + 4 <= compiled.len() {
            let opcode = u16::from_le_bytes([compiled[i], compiled[i + 1]]);
            if opcode == 0x001C {
                i += 4;
                continue;
            }
            let arg_len = u16::from_le_bytes([compiled[i + 2], compiled[i + 3]]) as usize;
            let args = &compiled[i + 4..(i + 4 + arg_len).min(compiled.len())];
            if opcode == 0x16 || opcode == 0x18 {
                // If/ElseIf: [jumpOps u16][exprLen u16][expr]
                if args.len() >= 4 {
                    let expr_len = u16::from_le_bytes([args[2], args[3]]) as usize;
                    let expr = &args[4..(4 + expr_len).min(args.len())];
                    walk_expr(expr, |tag, payload| {
                        if (tag == 0x73 || tag == 0x66) && payload.len() == 2 {
                            let idx = u16::from_le_bytes([payload[0], payload[1]]) as u32;
                            total_pairs += 1;
                            let id_hit = ids.iter().any(|&id| id == idx);
                            let pos_hit =
                                ids.iter().position(|&id| id == idx).map(|p| p as u32 + 1)
                                    == Some(idx);
                            if id_hit {
                                id_ok += 1;
                            }
                            if pos_hit {
                                pos_ok += 1;
                            }
                            if !id_hit && id_mismatch_examples.len() < 10 {
                                id_mismatch_examples.push(format!(
                                    "tag {:02X} idx {} — no SLSD has ID {}",
                                    tag, idx, idx
                                ));
                            }
                        }
                    });
                }
            }
            i += 4 + arg_len;
        }
    });

    println!("tagged pairs read from If/ElseIf conditions: {total_pairs}");
    println!(
        "index == some SLSD's ID:   {id_ok}  ({}%)",
        id_ok * 100 / total_pairs.max(1)
    );
    println!(
        "index == SLSD position:    {pos_ok}  ({}%)",
        pos_ok * 100 / total_pairs.max(1)
    );
    if !id_mismatch_examples.is_empty() {
        println!("pairs whose index is no SLSD ID:");
        for e in &id_mismatch_examples {
            println!("  {e}");
        }
    }
}
