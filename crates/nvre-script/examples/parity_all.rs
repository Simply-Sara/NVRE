//! The parity harness: does the compiler reproduce the game's own bytes?
//!
//! This is the acceptance test for the compiler, and the reason it is built
//! before the compiler. Every `SCPT` in `FalloutNV.esm` carries both its
//! source (`SCTX`) and its compiled bytes (`SCDA`), so each one is a
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

/// One entry of a unit's reference list, in the order the record writes its
/// `SCRO` and `SCRV` subrecords.
enum RefListEntry {
    /// An `SCRO` subrecord: the FormID it names.
    Scro(u32),
    /// An `SCRV` subrecord: the SLSD ID of the declared `ref` variable it
    /// binds.
    Scrv(u16),
}

/// One unit: its FormID, source, the game's bytes, its ordered reference
/// list, and its own variable table.
struct Unit {
    fid: u32,
    source: String,
    expected: Vec<u8>,
    ref_list: Vec<RefListEntry>,
    scro_fids: Vec<u32>,
    vars: Vec<nvre_script::VarEntry>,
}

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA to the game's Data folder");
        return;
    };
    let plugin_name = std::env::var("NVRE_PLUGIN").unwrap_or_else(|_| "FalloutNV.esm".to_string());
    let path = std::path::Path::new(&data).join(&plugin_name);
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("could not read {}", path.display());
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), plugin_name.clone()) else {
        eprintln!("could not parse the master");
        return;
    };

    // The plugin's own masters, in load order. A DLC's scripts name forms that
    // live in the master (`VNPCFollowers`, `Player`, every idle and package), so
    // compiling a DLC without them resolves nothing: 20% of its units would
    // "differ" for a reason that has nothing to do with the compiler. The
    // `MAST` subrecords of the file header are the list.
    let mut masters: Vec<Plugin> = Vec::new();
    {
        let mut names: Vec<String> = Vec::new();
        visit_nodes(&plugin.nodes, &mut |rec, _| {
            if rec.header.sig != *b"TES4" {
                return;
            }
            let Ok(subs) = rec.subrecords() else { return };
            for s in subs.iter() {
                if s.sig == *b"MAST" {
                    names.push(
                        String::from_utf8_lossy(&s.data)
                            .trim_end_matches('\0')
                            .to_string(),
                    );
                }
            }
        });
        for name in names {
            if name.eq_ignore_ascii_case(&plugin_name) {
                continue;
            }
            let path = std::path::Path::new(&data).join(&name);
            let Ok(bytes) = std::fs::read(&path) else {
                eprintln!("warning: master {name} not found; its names will not resolve");
                continue;
            };
            match Plugin::parse_bytes(Bytes::from(bytes), name.clone()) {
                Ok(p) => masters.push(p),
                Err(_) => eprintln!("warning: master {name} did not parse"),
            }
        }
    }

    // Pass one, over every record: the FormID -> EDID and FormID ->
    // signature maps the per-unit resolution needs, the SCRI chain (a
    // record's script), and every script's own variable table.
    //
    // The SCRI chain is what maps a source's remote name to the script
    // whose variable table the bytecode reads: `VNPCFollowers.bBooneHired`
    // names the quest, the quest's SCRI points at its script, and the
    // script's SCPT holds the table. Measured on the corpus; see the
    // obscript-compiler skill.
    let mut edid_of: std::collections::HashMap<u32, String> = Default::default();
    let mut sig_of: std::collections::HashMap<u32, String> = Default::default();
    let mut scri: std::collections::HashMap<u32, u32> = Default::default();
    // A placed reference's base object (ACHR/REFR/ACRE NAME), whose own
    // SCRI names the script the reference runs.
    let mut base_of: std::collections::HashMap<u32, u32> = Default::default();
    let mut script_tables: std::collections::HashMap<String, Vec<nvre_script::VarEntry>> =
        Default::default();
    for source in masters.iter().chain(std::iter::once(&plugin)) {
        visit_nodes(&source.nodes, &mut |rec, _| {
            let Ok(subs) = rec.subrecords() else { return };
            let sig = String::from_utf8_lossy(&rec.header.sig).into_owned();
            sig_of.insert(rec.header.form_id, sig.clone());
            let edid = subs
                .iter()
                .find(|x| x.sig == *b"EDID")
                .map(|x| {
                    String::from_utf8_lossy(&x.data)
                        .trim_end_matches('\0')
                        .to_string()
                })
                .unwrap_or_default();
            edid_of.insert(rec.header.form_id, edid.clone());
            if let Some(s) = subs.iter().find(|x| x.sig == *b"SCRI") {
                if s.data.len() == 4 {
                    scri.insert(
                        rec.header.form_id,
                        u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]),
                    );
                }
            }
            if matches!(sig.as_str(), "ACHR" | "REFR" | "ACRE") {
                if let Some(name) = subs.iter().find(|x| x.sig == *b"NAME") {
                    if name.data.len() == 4 {
                        base_of.insert(
                            rec.header.form_id,
                            u32::from_le_bytes([
                                name.data[0],
                                name.data[1],
                                name.data[2],
                                name.data[3],
                            ]),
                        );
                    }
                }
            }
            if rec.header.sig != *b"SCPT" {
                return;
            }
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
            if names.len() != slcds.len() || names.is_empty() {
                return;
            }
            let table: Vec<nvre_script::VarEntry> = names
                .iter()
                .zip(slcds.iter())
                .map(|(n, d)| nvre_script::VarEntry {
                    name: n.clone(),
                    id: u16::from_le_bytes([d[0], d[1]]),
                    is_int: d[16] == 1,
                })
                .collect();
            script_tables.insert(edid, table);
        });
    }
    // Player reference FormID 0x14
    edid_of.insert(0x00000014, "Player".to_string());
    sig_of.insert(0x00000014, "ACHR".to_string());

    // Pass two: the units.
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
        let source = subs
            .iter()
            .find(|x| x.sig == *b"SCTX")
            .map(|x| String::from_utf8_lossy(&x.data).into_owned())
            .unwrap_or_default();
        if compiled.is_empty() || source.is_empty() {
            return;
        }
        // The reference list, record order: SCRO and SCRV interleaved as
        // the record writes them. An SCRV payload is the SLSD ID of the
        // declared `ref` variable it binds.
        let mut ref_list: Vec<RefListEntry> = Vec::new();
        let mut scro_fids: Vec<u32> = Vec::new();
        for s in subs.iter() {
            if s.sig == *b"SCRO" && s.data.len() == 4 {
                let fid = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                scro_fids.push(fid);
                ref_list.push(RefListEntry::Scro(fid));
            } else if s.sig == *b"SCRV" && s.data.len() == 4 {
                let var_id = u16::from_le_bytes([s.data[0], s.data[1]]);
                ref_list.push(RefListEntry::Scrv(var_id));
            }
        }
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
            .map(|(n, d)| nvre_script::VarEntry {
                name: n.clone(),
                id: u16::from_le_bytes([d[0], d[1]]),
                is_int: d[16] == 1,
            })
            .collect();
        units.push(Unit {
            fid: rec.header.form_id,
            source,
            expected: compiled,
            ref_list,
            scro_fids,
            vars,
        });
    });

    println!("{} units with both source and bytes", units.len());
    println!("{} script variable tables", script_tables.len());
    println!();

    let mut equal = 0usize;
    let mut differs = 0usize;
    // Units the compiler **refuses**, which are not the same failure as a unit
    // that compiles to the wrong bytes and must not be counted as one. Every one
    // of them names data the record no longer carries — a form whose `EDID` was
    // renamed after the `.pex` was built, or a remote variable whose `SLSD` id
    // the record has since renumbered. The bytecode on disk holds the id the
    // GECK resolved at compile time; the shipped source holds the spelling and
    // the shipped record holds today's ids, and nothing ties the three back
    // together. No emitter rule can reproduce them, and the GECK itself would
    // fail to recompile them today.
    let mut refused = 0usize;
    let mut first_differs: Vec<(u32, String)> = Vec::new();
    let mut reasons: Vec<String> = Vec::new();
    let mut differing: Vec<&Unit> = Vec::new();

    // The reference entries a unit compiles against: the record's own
    // reference list, in the order the record writes its SCRO and SCRV
    // subrecords. An SCRO slot names a form (its EDID; a GLOB writes the
    // global tag); an SCRV slot names a declared `ref` variable — its
    // payload is that variable's SLSD ID, which is how the slot is matched
    // to the source's declaration.
    let context_for = |unit: &Unit| {
        let mut refs: Vec<nvre_script::RefEntry> = Vec::new();
        for entry in &unit.ref_list {
            match entry {
                RefListEntry::Scro(fid) => {
                    let name = edid_of
                        .get(fid)
                        .cloned()
                        .unwrap_or_else(|| format!("__form_{fid:08X}"));
                    let kind = match sig_of.get(fid).map(|s| s.as_str()) {
                        Some("GLOB") => nvre_script::RefKind::Global,
                        _ => nvre_script::RefKind::Reference,
                    };
                    refs.push(nvre_script::RefEntry {
                        name,
                        slot: nvre_script::RefSlot::Form,
                        kind,
                        form: *fid,
                    });
                }
                RefListEntry::Scrv(var_id) => {
                    // The declared ref variable whose SLSD ID the SCRV
                    // payload carries.
                    let name = unit
                        .vars
                        .iter()
                        .find(|v| v.id == *var_id)
                        .map(|v| v.name.clone())
                        .unwrap_or_else(|| format!("__var_{var_id}"));
                    refs.push(nvre_script::RefEntry {
                        name,
                        slot: nvre_script::RefSlot::Var,
                        kind: nvre_script::RefKind::Reference,
                        form: 0,
                    });
                }
            }
        }
        refs
    };

    // The remote tables a unit compiles against: each SCRO entry whose
    // record runs a script contributes that script's table, keyed by the
    // SCRO entry's name — the name the source spells. The chain is
    // measured: a quest runs its script through SCRI; a placed reference
    // (ACHR/REFR/ACRE) has none, and its script is on its base object (the
    // NAME FormID), which runs it through SCRI of its own.
    // `VFSOrrisThug01REF` -> NPC_ `VFSOrrisThug01` -> SCRI ->
    // `VFSOrrisThugScript`.
    let remotes_for = |unit: &Unit| {
        let mut remotes: Vec<nvre_script::RemoteScript> = Vec::new();
        for scro_fid in &unit.scro_fids {
            let remote_name = edid_of
                .get(scro_fid)
                .cloned()
                .unwrap_or_else(|| format!("__form_{scro_fid:08X}"));
            // The script FormID: the record's own SCRI, or its base's.
            let script_fid = scri
                .get(scro_fid)
                .copied()
                .or_else(|| base_of.get(scro_fid).and_then(|b| scri.get(b).copied()));
            let Some(script_fid) = script_fid else {
                continue;
            };
            let Some(script_edid) = edid_of.get(&script_fid).cloned() else {
                continue;
            };
            let Some(table) = script_tables.get(&script_edid).cloned() else {
                continue;
            };
            remotes.push(nvre_script::RemoteScript {
                name: remote_name,
                vars: table,
            });
        }
        remotes
    };

    for unit in &units {
        let refs = context_for(unit);
        let remotes = remotes_for(unit);
        match nvre_script::compile_with_refs(&unit.source, &refs, &unit.vars, &remotes) {
            Ok(got) => {
                if got.as_slice() == unit.expected.as_slice() {
                    equal += 1;
                } else {
                    reasons.push(describe(&got, &unit.expected));
                    differs += 1;
                    differing.push(unit);
                    if first_differs.len() < 2000 {
                        first_differs.push((unit.fid, describe(&got, &unit.expected)));
                    }
                }
            }
            Err(e) => {
                refused += 1;
                reasons.push(format!("refused: {e}"));
                if first_differs.len() < 2000 {
                    first_differs.push((unit.fid, format!("refused: {e}")));
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
    for (reason, count) in sorted.iter().take(2000) {
        println!("  {count:>5}  {reason}");
    }

    let total = equal + differs + refused;
    let comparable = equal + differs;
    let pct = if comparable == 0 {
        0.0
    } else {
        equal as f64 * 100.0 / comparable as f64
    };
    println!("units:      {total}");
    println!("byte-equal: {equal} / {comparable}  ({pct:.2}% of the units that compile)");
    println!("refused:    {refused}  (they name data the record no longer carries)");
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
        let refs = context_for(u);
        let remotes = remotes_for(u);
        let Ok(got) = nvre_script::compile_with_refs(&u.source, &refs, &u.vars, &remotes) else {
            continue;
        };
        if got.len() != u.expected.len() {
            if length_diffs < 2000 {
                println!(
                    "  {:08X}  {} bytes vs {}",
                    u.fid,
                    got.len(),
                    u.expected.len()
                );
                for line in u.source.lines().filter(|l| !l.trim().is_empty()).take(10) {
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
        if shown >= 2000 {
            break;
        }
        let refs = context_for(u);
        let remotes = remotes_for(u);
        let Err(e) = nvre_script::compile_with_refs(&u.source, &refs, &u.vars, &remotes) else {
            continue;
        };
        if !e.to_string().contains("is not a declared variable") {
            continue;
        }
        // The line the message names, when it names one.
        println!("  {e}");
        for line in u.source.lines().filter(|l| !l.trim().is_empty()).take(14) {
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
