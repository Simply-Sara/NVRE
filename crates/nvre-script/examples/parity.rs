//! The parity harness: does the compiler reproduce the game's own bytes?
//!
//! This is the acceptance test for the compiler, and the reason it is built
//! before the compiler. Every `SCPT` in `FalloutNV.esm` carries both its
//! source (`SCTX`) and its compiled body (`SCDA`), so each one is a
//! source→bytecode pair the compiler can be checked against. Compiling the
//! source and comparing with the bytes the game wrote for it is a test that
//! cannot be satisfied by a spec written from memory: either the bytes match
//! or they do not.
//!
//! The harness reports a count, not a pass/fail. That is deliberate — the
//! number is the thing to watch climb, and the units that do not match are
//! listed so each one can be explained rather than averaged away.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example parity
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA to the game's Data folder");
        return;
    };
    let path = std::path::Path::new(&data).join("FalloutNV.esm");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("could not read {}", path.display());
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), "FalloutNV.esm".to_string()) else {
        eprintln!("could not parse the master");
        return;
    };

    // Collect FormID -> EDID map from plugin for SCRO resolution
    let mut formid_to_edid: std::collections::HashMap<u32, String> =
        std::collections::HashMap::new();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        let Ok(subs) = rec.subrecords() else { return };
        if let Some(edid_sub) = subs.iter().find(|x| x.sig == *b"EDID") {
            let edid = String::from_utf8_lossy(&edid_sub.data)
                .trim_end_matches('\0')
                .to_string();
            formid_to_edid.insert(rec.header.form_id, edid);
        }
    });
    // Player reference FormID 0x14
    formid_to_edid.insert(0x00000014, "Player".to_string());

    let mut units: Vec<(
        u32,
        String,
        Vec<u8>,
        Vec<String>,
        Vec<nvre_script::VarEntry>,
    )> = Vec::new();
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
        if !compiled.is_empty() && !source.is_empty() {
            // Collect SCRO FormIDs and map to EDIDs
            let scro_names: Vec<String> = subs
                .iter()
                .filter(|x| x.sig == *b"SCRO" && x.data.len() == 4)
                .map(|x| {
                    let fid = u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]);
                    formid_to_edid
                        .get(&fid)
                        .cloned()
                        .unwrap_or_else(|| format!("__form_{fid:08X}"))
                })
                .collect();
            // The record's own variable table: SCVR names paired positionally
            // with SLSD entries, whose first word is the ID the bytecode
            // indexes by. SLSD byte 16 is the type: 0 float/ref, 1 integer.
            let names: Vec<String> = subs
                .iter()
                .filter(|x| x.sig == *b"SCVR")
                .map(|x| {
                    String::from_utf8_lossy(&x.data)
                        .trim_end_matches('\0')
                        .to_string()
                })
                .collect();
            let slcds: Vec<&[u8]> = subs
                .iter()
                .filter(|x| x.sig == *b"SLSD" && x.data.len() >= 20)
                .map(|x| x.data.as_slice())
                .collect();
            let vars: Vec<nvre_script::VarEntry> = names
                .iter()
                .zip(slcds.iter())
                .map(|(name, d)| nvre_script::VarEntry {
                    name: name.clone(),
                    id: u16::from_le_bytes([d[0], d[1]]),
                    is_int: d[16] == 1,
                })
                .collect();
            units.push((rec.header.form_id, source, compiled, scro_names, vars));
        }
    });

    println!("{} units with both source and bytes", units.len());
    println!();

    let mut equal = 0usize;
    let mut differs = 0usize;
    let mut first_differs: Vec<(u32, String)> = Vec::new();
    let mut reasons: Vec<String> = Vec::new();
    let mut differing: Vec<&(
        u32,
        String,
        Vec<u8>,
        Vec<String>,
        Vec<nvre_script::VarEntry>,
    )> = Vec::new();

    for unit in &units {
        let (fid, source, expected, scro_names, vars) =
            (unit.0, &unit.1, &unit.2, &unit.3, &unit.4);
        match nvre_script::compile_with_refs(source.as_str(), scro_names, vars) {
            Ok(got) => {
                if got.as_slice() == expected.as_slice() {
                    equal += 1;
                } else {
                    reasons.push(describe(&got, expected));
                    differs += 1;
                    differing.push(unit);
                    if first_differs.len() < 20 {
                        first_differs.push((fid, describe(&got, expected)));
                    }
                }
            }
            Err(e) => {
                differs += 1;
                reasons.push(format!("compile error: {e}"));
                differing.push(unit);
                if first_differs.len() < 20 {
                    first_differs.push((fid, format!("compile error: {e}")));
                }
            }
        }
    }

    // Why each unit failed, grouped. A count per reason says what to emit next,
    // rather than whatever unit happens to sort first.
    let mut counted: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for r in &reasons {
        // The message's leading shape, so the same kind of failure groups
        // together even when the names in it differ.
        // The whole message, so a specific reason is visible rather than
        // everything collapsing into `compile error`.
        *counted.entry(r.clone()).or_insert(0) += 1;
    }
    println!("why they differ:");
    let mut sorted: Vec<(&String, &usize)> = counted.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (reason, count) in sorted.iter().take(20) {
        println!("  {count:>5}  {reason}");
    }

    let total = equal + differs;
    let pct = if total == 0 {
        0.0
    } else {
        equal as f64 * 100.0 / total as f64
    };
    println!("byte-equal: {equal} / {total}  ({pct:.2}%)");
    println!("differs:    {differs}");
    // A few source lines behind the most common reasons, so the failure can be
    // read rather than guessed from its message alone.
    println!();
    // The units that compile but differ only by a couple of bytes: those are
    // a real encoding bug rather than a missing statement type, and they are
    // worth reading because a length mismatch localises the fault.
    println!("the units that compile but differ in length:");
    let mut length_diffs = 0;
    for u in &differing {
        let Ok(got) = nvre_script::compile_with_refs(&u.1, &u.3, &u.4) else {
            continue;
        };
        if got.len() != u.2.len() {
            if length_diffs < 5 {
                println!("  {:08X}  {} bytes vs {}", u.0, got.len(), u.2.len());
                for line in u.1.lines().filter(|l| !l.trim().is_empty()).take(10) {
                    println!("      | {line}");
                }
            }
            length_diffs += 1;
        }
    }
    println!("  ({length_diffs} units compile but have the wrong length)");

    println!();
    println!("a few failing source lines:");
    let mut shown = 0;
    for u in &differing {
        if shown >= 6 {
            break;
        }
        let Err(e) = nvre_script::compile_with_refs(&u.1, &u.3, &u.4) else {
            continue;
        };
        if !e.to_string().contains("is not a declared variable") {
            continue;
        }
        // The line the message names, when it names one.
        println!("  {e}");
        for line in u.1.lines().filter(|l| !l.trim().is_empty()).take(14) {
            println!("      | {line}");
        }
        shown += 1;
        println!();
    }

    println!();
    if !first_differs.is_empty() {
        println!("the first {} that differ:", first_differs.len());
        for (fid, why) in &first_differs {
            println!("  {fid:08X}  {why}");
        }
    }
}

/// What differs, in a form a person can act on.
fn describe(got: &[u8], expected: &[u8]) -> String {
    if got.len() != expected.len() {
        return format!(
            "length {} vs {} ({} byte(s) {})",
            got.len(),
            expected.len(),
            got.len().abs_diff(expected.len()),
            if got.len() < expected.len() {
                "short"
            } else {
                "long"
            }
        );
    }
    // Same length: the first offset that differs, and the bytes either side.
    for (i, (a, b)) in got.iter().zip(expected.iter()).enumerate() {
        if a != b {
            let lo = i.saturating_sub(4);
            let hi = (i + 8).min(got.len());
            return format!(
                "first difference at {i}: got {:02X?} expected {:02X?}",
                &got[lo..hi],
                &expected[lo..hi]
            );
        }
    }
    "identical".to_string()
}
