//! Probe: remote script variables in every source position, measured.
//!
//! The remote chain, as the corpus settles it: a source name like
//! `VNPCFollowers` or `VFSOrrisThug01REF` is NOT itself a script. It is a
//! quest or a placed form, and it reaches a script's variable table through
//! the record the unit's reference list names:
//!
//!   reference entry -> that record -> its SCRI (quest script) or SCRV
//!   (object script) subrecord -> the SCPT at that FormID -> its
//!   SCVR/SLSD table. A placed reference (REFR/ACHR/ACRE) carries no script
//!   of its own: follow its NAME (base form) to the base object's SCRV.
//!
//! The reference INDEX space the bytecode's `72 <refIdx>` reads is the
//! SCPT record's own reference list in RECORD ORDER — every `SCRO` and
//! `SCRV` subrecord, interleaved as they sit in the record, 1-based. An
//! `SCRV` entry's payload is the SLSD id of the `ref` variable it names,
//! which is how a declared `ref ThisThug` is reached: not by a declared-
//! refs-first count, but by the SCRV entry carrying that variable's id.
//! (Measured on VFSOrrisThugScript: its list is SCRO VFSOrrisREF, SCRO
//! Thug01..04, SCRV 3, SCRV 6, SCRO Player, SCRO ..., SCRO VMS19 — and the
//! game writes refIdx 1 for VFSOrrisREF, 6 for ThisThug, 10 for VMS19.)
//!
//! A GLOB record is not a script: it is a global variable, and the bytecode
//! reads it as `47 <refIdx>` with no compound — see Q4.
//!
//! This probe answers, with unit name + source line + exact bytes:
//!
//!   Q1 SET DESTINATION: the shape of `set QuestScript.var to X`, and
//!      whether a remote FLOAT var destination writes tag 66.
//!   Q2 PLACED-REF REMOTES: how `PlacedREF.FakingDeath` resolves — the
//!      record chain REFR/ACHR -> NAME(base) -> SCRV/SCRI -> SCPT, and the
//!      index space the refIdx counts.
//!   Q3 COMMAND ARGUMENT: any corpus call whose argument is a remote var
//!      (ShowMessage format values are the known case; anything else?).
//!   Q4 THE `47` DESTINATION: when a set destination writes `47 <refIdx>`
//!      instead of the `72 <refIdx> <tag> <id>` compound — hypothesis: the
//!      destination is a GLOBAL variable (a GLOB form in the reference list).
//!
//! Plus the original deliverable: for the first 30 units that use remote
//! variables, every attributed compound in the SCDA with the source line
//! that names it, and the census over the whole corpus.
//!
//! Attribution: every dotted name `A.b` in the source (A not a declared
//! non-ref local) predicts exact compound bytes — refIdx from A's position
//! in the record-order reference list, tag and varId from b's SLSD entry in
//! the script the chain reaches. An actual compound is attributed when its
//! bytes equal a prediction. Compounds no source use predicts are dumped as
//! a sample (usually a `72 <ref>` argument adjacent to a `73 <var>`
//! argument, not one compound).
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_remote_forms
//! NVRE_UNIT=<substring> cargo run ... --example probe_remote_forms
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;
use nvre_script::parse::VarEntry;

const TAG_INT: u8 = 0x73; // 's'
const TAG_FLOAT: u8 = 0x66; // 'f'
const TAG_REF: u8 = 0x72; // 'r'
const TAG_GLOBAL: u8 = 0x47; // 'G'

#[derive(Clone)]
struct RecInfo {
    sig: [u8; 4],
    edid: String,
    scri: Option<u32>,
    scrv: Option<u32>,
    base: Option<u32>,
}

/// One entry of a unit's reference list, in record order.
struct RefEntry {
    /// The FormID the entry names.
    fid: u32,
    /// For an SCRV entry: the SLSD id of the `ref` variable it names.
    slsd_id: Option<u16>,
}

struct Unit {
    form_id: u32,
    edid: String,
    source: String,
    compiled: Vec<u8>,
    /// The reference list, record order: SCRO and SCRV interleaved.
    refs: Vec<RefEntry>,
    /// Every declared local variable name (int and float), for excluding
    /// local uses from the remote pattern.
    local_vars: Vec<String>,
}

/// A dotted name `A.b` in the source, on line `line_no`.
#[derive(Clone)]
struct Use {
    line_no: usize,
    line: String,
    script_part: String,
    var_part: String,
}

/// A compound the bytecode actually contains: `72 i0 i1 t d0 d1`.
#[derive(Clone, Copy)]
struct Actual {
    at: usize,
    opcode: u16,
    /// The compound's offset inside the instruction's argument bytes.
    off: usize,
    idx: u16,
    tag: u8,
    id: u16,
}

fn position(opcode: u16, off: usize) -> &'static str {
    match opcode {
        0x0015 if off == 0 => "set DESTINATION",
        0x0015 => "set VALUE (RHS)",
        0x1059 => "ShowMessage format value",
        0x0010 => "Begin parameter",
        0x0016 | 0x0018 => "expression operand (if/elseif)",
        op if op >= 0x1000 => "command argument",
        _ => "other",
    }
}

fn hex_ins(ins: &nvre_script::emit::Instruction) -> String {
    let mut v = ins.opcode.to_le_bytes().to_vec();
    v.extend((ins.args.len() as u16).to_le_bytes());
    v.extend(&ins.args);
    v.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_comment(line: &str) -> &str {
    match line.find(';') {
        Some(at) => &line[..at],
        None => line,
    }
}

fn fmt6(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

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
    let unit_filter = std::env::var("NVRE_UNIT").ok();

    // A placed reference: its record carries no script of its own.
    let is_placed_ref = |sig: [u8; 4]| {
        sig == *b"REFR" || sig == *b"ACHR" || sig == *b"ACRE" || sig == *b"PMIS" || sig == *b"PARW"
    };

    // ---- Pass A: every record, as the chain needs it ----------------------
    let mut recs: std::collections::HashMap<u32, RecInfo> = Default::default();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
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
        let four = |s: &[u8]| {
            subs.iter()
                .find(|x| x.sig == s && x.data.len() == 4)
                .map(|x| u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]))
        };
        recs.insert(
            rec.header.form_id,
            RecInfo {
                sig: rec.header.sig,
                edid,
                scri: four(b"SCRI"),
                scrv: four(b"SCRV"),
                base: four(b"NAME"),
            },
        );
    });

    // ---- Pass B: every script's variable table, by SCPT FormID ------------
    let mut tables: std::collections::HashMap<u32, Vec<VarEntry>> = Default::default();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
        let names: Vec<String> = subs
            .iter()
            .filter(|x| x.sig == *b"SCVR" && !x.data.is_empty())
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
        if names.len() != slcds.len() {
            return;
        }
        let vars = names
            .iter()
            .zip(slcds.iter())
            .map(|(name, d)| VarEntry {
                name: name.clone(),
                id: u16::from_le_bytes([d[0], d[1]]),
                is_int: d[16] == 1,
            })
            .collect();
        tables.insert(rec.header.form_id, vars);
    });

    // The chain: reference entry FormID -> record -> SCRI/SCRV -> SCPT.
    // An SCRV entry in a unit's own list is not followed here (it names a
    // ref variable, not a script-bearing form); the caller skips those.
    let chain = |start_fid: u32| -> (String, Option<u32>, String) {
        let mut steps = Vec::new();
        let mut fid = start_fid;
        let mut script_fid = None;
        let mut script_edid = String::new();
        for _ in 0..6 {
            let Some(r) = recs.get(&fid) else {
                steps.push(format!("[no record {fid:08X}]"));
                break;
            };
            steps.push(format!("{} {}({:08X})", r.sig_str(), r.edid, fid));
            if r.sig == *b"SCPT" {
                script_fid = Some(fid);
                script_edid = r.edid.clone();
                break;
            }
            if let Some(s) = r.scri {
                steps.push("--SCRI-->".to_string());
                fid = s;
                continue;
            }
            if let Some(s) = r.scrv {
                steps.push("--SCRV-->".to_string());
                fid = s;
                continue;
            }
            if is_placed_ref(r.sig) {
                if let Some(b) = r.base {
                    steps.push("--NAME(base)-->".to_string());
                    fid = b;
                    continue;
                }
            }
            break;
        }
        (steps.join(" "), script_fid, script_edid)
    };

    // ---- The units ---------------------------------------------------------
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
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        if compiled.is_empty() || source.is_empty() || edid.is_empty() {
            return;
        }
        // The reference list, record order. SCRO's payload is a FormID.
        // SCRV's payload is the SLSD id of the ref variable it names.
        let mut refs = Vec::new();
        let mut local_vars = Vec::new();
        for s in subs.iter() {
            if s.sig == *b"SCRO" && s.data.len() == 4 {
                refs.push(RefEntry {
                    fid: u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]),
                    slsd_id: None,
                });
            } else if s.sig == *b"SCRV" && s.data.len() == 4 {
                refs.push(RefEntry {
                    fid: 0,
                    slsd_id: Some(u16::from_le_bytes([s.data[0], s.data[1]])),
                });
            }
        }
        for line in source.lines() {
            let t = strip_comment(line).trim();
            let mut words = t.split_whitespace();
            let Some(first) = words.next() else { continue };
            let Some(name) = words.next() else { continue };
            let name = name.trim_end_matches(';').to_string();
            if ["short", "long", "int", "float"]
                .iter()
                .any(|k| first.eq_ignore_ascii_case(k))
            {
                local_vars.push(name);
            }
        }
        units.push(Unit {
            form_id: rec.header.form_id,
            edid,
            source,
            compiled,
            refs,
            local_vars,
        });
    });

    println!("{} SCPT units with source and bytecode", units.len());
    println!(
        "{} records in the master, {} script variable tables",
        recs.len(),
        tables.len()
    );

    // How many units have SCRV entries in their reference list at all?
    let with_scrv = units
        .iter()
        .filter(|u| u.refs.iter().any(|r| r.slsd_id.is_some()))
        .count();
    println!(
        "units whose reference list carries SCRV entries (declared refs as list entries): {with_scrv}"
    );

    // Per-unit SLSD/SCVR tables, keyed by the unit's own FormID: needed to
    // name an SCRV entry's ref variable (the SCRV payload is that variable's
    // SLSD id) and to resolve a source name against it.
    let mut unit_var_tables: std::collections::HashMap<u32, Vec<VarEntry>> = Default::default();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
        let names: Vec<String> = subs
            .iter()
            .filter(|x| x.sig == *b"SCVR" && !x.data.is_empty())
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
        if names.len() != slcds.len() {
            return;
        }
        let vars = names
            .iter()
            .zip(slcds.iter())
            .map(|(name, d)| VarEntry {
                name: name.clone(),
                id: u16::from_le_bytes([d[0], d[1]]),
                is_int: d[16] == 1,
            })
            .collect();
        unit_var_tables.insert(rec.header.form_id, vars);
    });

    // Resolve a name in the unit's reference space: record order, 1-based.
    // An SCRO entry resolves by its record's EDID; an SCRV entry by the
    // declared ref variable whose SLSD id it carries.
    let resolve = |u: &Unit, name: &str| -> Option<u16> {
        let own = unit_var_tables.get(&u.form_id);
        for (i, r) in u.refs.iter().enumerate() {
            match r.slsd_id {
                None => {
                    if let Some(info) = recs.get(&r.fid) {
                        if info.edid.eq_ignore_ascii_case(name) {
                            return Some((i + 1) as u16);
                        }
                    }
                }
                Some(slsd) => {
                    if let Some(v) = own.and_then(|t| t.iter().find(|v| v.id == slsd)) {
                        if v.name.eq_ignore_ascii_case(name) {
                            return Some((i + 1) as u16);
                        }
                    }
                }
            }
        }
        None
    };

    // Name what a refIdx points at.
    let name_idx = |u: &Unit, idx: u16| -> String {
        let i = idx as usize;
        if i >= 1 && i <= u.refs.len() {
            match u.refs[i - 1].slsd_id {
                None => {
                    let fid = u.refs[i - 1].fid;
                    match recs.get(&fid) {
                        Some(r) => format!(
                            "ref #{} = {:08X} {} {}  SCRI={:?} SCRV={:?} NAME(base)={:?}",
                            i,
                            fid,
                            r.sig_str(),
                            r.edid,
                            r.scri,
                            r.scrv,
                            r.base
                        ),
                        None => format!("ref #{} = {:08X} (no record)", i, fid),
                    }
                }
                Some(slsd) => format!("ref #{} = SCRV (ref var, SLSD id {slsd})", i),
            }
        } else {
            format!(
                "idx {} OUT OF SPACE (list has {} entries)",
                idx,
                u.refs.len()
            )
        }
    };

    // ---- Per unit: source uses and actual compounds -----------------------
    struct UnitData {
        uses: Vec<Use>,
        actuals: Vec<Actual>,
    }
    let mut all: Vec<(usize, UnitData)> = Vec::new();
    for (ui, u) in units.iter().enumerate() {
        let mut uses = Vec::new();
        for (li, line) in u.source.lines().enumerate() {
            for word in strip_comment(line).split_whitespace() {
                if word.starts_with('"') {
                    continue;
                }
                let w: String = word
                    .trim_matches(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
                    .to_string();
                let parts: Vec<&str> = w.split('.').collect();
                if parts.len() != 2 {
                    continue;
                }
                let (a, b) = (parts[0], parts[1]);
                if a.is_empty() || b.is_empty() || b.len() > 64 {
                    continue;
                }
                if u.local_vars.iter().any(|v| v.eq_ignore_ascii_case(a)) {
                    continue;
                }
                uses.push(Use {
                    line_no: li,
                    line: strip_comment(line).trim().to_string(),
                    script_part: a.to_string(),
                    var_part: b.to_string(),
                });
            }
        }
        let mut actuals = Vec::new();
        for ins in read_instructions(&u.compiled) {
            if ins.args.len() < 6 {
                continue;
            }
            for i in 0..=(ins.args.len() - 6) {
                if ins.args[i] == TAG_REF {
                    let tag = ins.args[i + 3];
                    if tag == TAG_INT || tag == TAG_FLOAT {
                        actuals.push(Actual {
                            at: ins.at,
                            opcode: ins.opcode,
                            off: i,
                            idx: u16::from_le_bytes([ins.args[i + 1], ins.args[i + 2]]),
                            tag,
                            id: u16::from_le_bytes([ins.args[i + 4], ins.args[i + 5]]),
                        });
                    }
                }
            }
        }
        if !uses.is_empty() || !actuals.is_empty() {
            all.push((ui, UnitData { uses, actuals }));
        }
    }

    let n_uses: usize = all.iter().map(|(_, d)| d.uses.len()).sum();
    let n_actuals: usize = all.iter().map(|(_, d)| d.actuals.len()).sum();
    println!(
        "units with a dotted remote-looking source use and/or an actual compound: {} (source uses: {n_uses}, actual compounds: {n_actuals})",
        all.len()
    );
    println!();

    // ---- Attribution --------------------------------------------------------
    struct Attributed {
        unit: usize,
        use_: Use,
        #[allow(dead_code)]
        idx: u16,
        chain: String,
        script_edid: String,
        var_name: String,
        var_id: u16,
        var_is_int: bool,
        act: Actual,
    }
    let mut attributed: Vec<Attributed> = Vec::new();
    let mut unattributed_total = 0usize;
    let mut unattributed_sample: Vec<(usize, Actual)> = Vec::new();

    for (ui, d) in &all {
        let u = &units[*ui];
        let mut used = vec![false; d.actuals.len()];
        for use_ in &d.uses {
            let Some(idx) = resolve(u, &use_.script_part) else {
                continue;
            };
            let Some(entry) = u.refs.get(idx as usize - 1) else {
                continue;
            };
            // An SCRV entry names a ref VARIABLE, not a script-bearing form:
            // a compound through it is the ref-var destination form, not a
            // remote script read. Skip it for remote attribution.
            if entry.slsd_id.is_some() {
                continue;
            }
            let (steps, script_fid, script_edid) = chain(entry.fid);
            let Some(script_fid) = script_fid else {
                continue;
            };
            let Some(table) = tables.get(&script_fid) else {
                continue;
            };
            let Some(ve) = table
                .iter()
                .find(|v| v.name.eq_ignore_ascii_case(&use_.var_part))
            else {
                continue;
            };
            let tag = if ve.is_int { TAG_INT } else { TAG_FLOAT };
            for (ai, a) in d.actuals.iter().enumerate() {
                if used[ai] {
                    continue;
                }
                if a.idx == idx && a.tag == tag && a.id == ve.id {
                    used[ai] = true;
                    attributed.push(Attributed {
                        unit: *ui,
                        use_: use_.clone(),
                        idx,
                        chain: steps,
                        script_edid,
                        var_name: ve.name.clone(),
                        var_id: ve.id,
                        var_is_int: ve.is_int,
                        act: *a,
                    });
                    break;
                }
            }
        }
        for (ai, a) in d.actuals.iter().enumerate() {
            if !used[ai] {
                unattributed_total += 1;
                if unattributed_sample.len() < 40 {
                    unattributed_sample.push((*ui, *a));
                }
            }
        }
    }

    println!(
        "attributed compounds (a source use predicted the exact bytes): {}; unattributed actuals: {unattributed_total}",
        attributed.len()
    );
    println!();

    // ---- Focused dump for NVRE_UNIT ----------------------------------------
    if let Some(filter) = &unit_filter {
        println!("=== focused: units matching {filter} ===\n");
        for (ui, d) in &all {
            let u = &units[*ui];
            if !u
                .edid
                .to_ascii_lowercase()
                .contains(&filter.to_ascii_lowercase())
            {
                continue;
            }
            println!("--- {} (formID {:08X}) ---", u.edid, u.form_id);
            println!(
                "    reference list, record order ({} entries):",
                u.refs.len()
            );
            for (i, r) in u.refs.iter().enumerate() {
                match r.slsd_id {
                    None => match recs.get(&r.fid) {
                        Some(ri) => println!(
                            "    [{}] {:08X} {} {}  SCRI={:?} SCRV={:?} NAME(base)={:?}",
                            i + 1,
                            r.fid,
                            ri.sig_str(),
                            ri.edid,
                            ri.scri,
                            ri.scrv,
                            ri.base
                        ),
                        None => println!("    [{}] {:08X} (no record)", i + 1, r.fid),
                    },
                    Some(slsd) => println!("    [{}] SCRV ref var (SLSD id {})", i + 1, slsd),
                }
            }
            println!("    source uses (dotted names, comments stripped):");
            for s in &d.uses {
                println!("      line {}: {}", s.line_no + 1, s.line);
            }
            println!("    actual compounds in bytecode:");
            let instructions = read_instructions(&u.compiled);
            for a in &d.actuals {
                let ins = instructions.iter().find(|i| i.at == a.at);
                println!(
                    "      @{:<5} op 0x{:04X} off {}: 72 {} {:02X} {}  [{}]",
                    a.at,
                    a.opcode,
                    a.off,
                    a.idx,
                    a.tag,
                    a.id,
                    position(a.opcode, a.off)
                );
                println!("        refIdx {}: {}", a.idx, name_idx(u, a.idx));
                if let Some(ins) = ins {
                    println!("        instr: {}", hex_ins(ins));
                }
            }
            println!();
        }
    }

    // =========================================================================
    // Q1: set destination
    // =========================================================================
    println!("=== Q1: set destination — `set A.b to X` ===\n");

    // Census of Set destination shapes corpus-wide.
    let mut dest_shapes: std::collections::BTreeMap<String, usize> = Default::default();
    for u in &units {
        for ins in read_instructions(&u.compiled) {
            if ins.opcode != 0x0015 || ins.args.is_empty() {
                continue;
            }
            let d0 = ins.args[0];
            let shape = match d0 {
                0x73 => "73 <local int var id>",
                0x66 => "66 <local float/ref var id>",
                0x72 if ins.args.len() >= 6
                    && (ins.args[3] == TAG_INT || ins.args[3] == TAG_FLOAT) =>
                {
                    "72 <refIdx> <tag> <varId>  (compound: remote var dest)"
                }
                0x72 => "72 <refIdx>  (plain ref dest)",
                0x47 => "47 <refIdx>  (global dest)",
                _ => "other",
            };
            *dest_shapes.entry(shape.to_string()).or_insert(0) += 1;
        }
    }
    println!("Set destination shapes, corpus census:");
    let mut ds: Vec<(String, usize)> = dest_shapes.into_iter().collect();
    ds.sort_by_key(|r| std::cmp::Reverse(r.1));
    for (k, c) in &ds {
        println!("  {c:>6}  {k}");
    }
    println!();

    // Every dotted set TARGET whose chain resolves, byte-matched against the
    // unit's compound-destination Sets.
    let mut q1_int_ok = 0usize;
    let mut q1_float_ok = 0usize;
    let mut q1_int_bad = 0usize;
    let mut q1_float_bad = 0usize;
    let mut q1_nochain = 0usize;
    let mut q1_int_ev: Vec<String> = Vec::new();
    let mut q1_float_ev: Vec<String> = Vec::new();
    let mut q1_nochain_ev: Vec<String> = Vec::new();
    for (ui, d) in &all {
        let u = &units[*ui];
        // Dotted set TARGETS in source order.
        let mut dotted: Vec<&Use> = Vec::new();
        for use_ in &d.uses {
            let Some(line) = u.source.lines().nth(use_.line_no) else {
                continue;
            };
            let t = strip_comment(line).trim();
            let low = t.to_ascii_lowercase();
            let is_set = low.starts_with("set ") || low.starts_with("let ");
            let is_target = is_set
                && t[4..]
                    .split_once(" to ")
                    .map(|(target, _)| {
                        target.trim().to_ascii_lowercase().contains(&format!(
                            "{}.{}",
                            use_.script_part.to_ascii_lowercase(),
                            use_.var_part.to_ascii_lowercase()
                        ))
                    })
                    .unwrap_or(false);
            if is_target {
                dotted.push(use_);
            }
        }
        // All Sets whose destination is a compound.
        let compound_sets: Vec<_> = read_instructions(&u.compiled)
            .into_iter()
            .filter(|i| {
                i.opcode == 0x0015
                    && i.args.len() >= 6
                    && i.args[0] == TAG_REF
                    && (i.args[3] == TAG_INT || i.args[3] == TAG_FLOAT)
            })
            .collect();
        for use_ in &dotted {
            let Some(idx) = resolve(u, &use_.script_part) else {
                q1_nochain += 1;
                if q1_nochain_ev.len() < 8 {
                    q1_nochain_ev.push(format!(
                        "{} line {}: {}  (script part `{}` is not in the unit's reference list)",
                        u.edid,
                        use_.line_no + 1,
                        use_.line,
                        use_.script_part
                    ));
                }
                continue;
            };
            let Some(entry) = u.refs.get(idx as usize - 1) else {
                q1_nochain += 1;
                continue;
            };
            if entry.slsd_id.is_some() {
                q1_nochain += 1;
                if q1_nochain_ev.len() < 8 {
                    q1_nochain_ev.push(format!(
                        "{} line {}: {}  (script part `{}` is a DECLARED REF — reached as an SCRV entry, not a script-bearing form)",
                        u.edid,
                        use_.line_no + 1,
                        use_.line,
                        use_.script_part
                    ));
                }
                continue;
            }
            let (steps, script_fid, _) = chain(entry.fid);
            let Some(script_fid) = script_fid else {
                q1_nochain += 1;
                if q1_nochain_ev.len() < 8 {
                    q1_nochain_ev.push(format!(
                        "{} line {}: {}  (chain from {} reaches no script: {})",
                        u.edid,
                        use_.line_no + 1,
                        use_.line,
                        use_.script_part,
                        steps
                    ));
                }
                continue;
            };
            let Some(table) = tables.get(&script_fid) else {
                q1_nochain += 1;
                continue;
            };
            let Some(ve) = table
                .iter()
                .find(|v| v.name.eq_ignore_ascii_case(&use_.var_part))
            else {
                q1_nochain += 1;
                continue;
            };
            let tag = if ve.is_int { TAG_INT } else { TAG_FLOAT };
            let predict = [
                TAG_REF,
                idx.to_le_bytes()[0],
                idx.to_le_bytes()[1],
                tag,
                ve.id.to_le_bytes()[0],
                ve.id.to_le_bytes()[1],
            ];
            // Byte-exact match against ANY compound-destination Set.
            let matched = compound_sets
                .iter()
                .find(|s| s.args.len() >= 6 && s.args[0..6] == predict[..]);
            let is_float = !ve.is_int;
            if let Some(s) = matched {
                let full = hex_ins(s);
                let entry = format!(
                    "{} (formID {:08X}) line {}: {}\n      target {}.{} -> refIdx {} ({}), remote var {} id {} ({}), chain: {}\n      predicted dest: {}\n      actual Set:     {}",
                    u.edid,
                    u.form_id,
                    use_.line_no + 1,
                    use_.line,
                    use_.script_part,
                    use_.var_part,
                    idx,
                    steps,
                    use_.var_part,
                    ve.id,
                    if ve.is_int { "int" } else { "float" },
                    steps,
                    fmt6(&predict),
                    full
                );
                if is_float {
                    q1_float_ok += 1;
                    if q1_float_ev.len() < 12 {
                        q1_float_ev.push(entry);
                    }
                } else {
                    q1_int_ok += 1;
                    if q1_int_ev.len() < 15 {
                        q1_int_ev.push(entry);
                    }
                }
            } else if is_float {
                q1_float_bad += 1;
                if q1_float_bad <= 10 {
                    println!(
                        "  MISMATCH(float) {} line {}: {}\n      predicted: {} (chain: {})\n      unit compound Sets: {}",
                        u.edid,
                        use_.line_no + 1,
                        use_.line,
                        fmt6(&predict),
                        steps,
                        compound_sets
                            .iter()
                            .map(hex_ins)
                            .collect::<Vec<_>>()
                            .join(" | ")
                    );
                }
            } else {
                q1_int_bad += 1;
                if q1_int_bad <= 25 {
                    println!(
                        "  MISMATCH(int) {} (formID {:08X}) line {}: {}\n      target {}.{} -> refIdx {} predicted {} (chain: {})\n      remote var {} id {} (int), but no Set starts with those bytes. Compound Sets in unit: {}",
                        u.edid,
                        u.form_id,
                        use_.line_no + 1,
                        use_.line,
                        use_.script_part,
                        use_.var_part,
                        idx,
                        fmt6(&predict),
                        steps,
                        use_.var_part,
                        ve.id,
                        compound_sets
                            .iter()
                            .map(hex_ins)
                            .collect::<Vec<_>>()
                            .join(" | ")
                    );
                }
            }
        }
    }
    println!(
        "dotted set targets with a resolved chain: int destinations byte-exact: {q1_int_ok} (mismatch: {q1_int_bad}); FLOAT destinations byte-exact: {q1_float_ok} (mismatch: {q1_float_bad}); unresolvable: {q1_nochain}"
    );
    println!("int destination evidence (dest compound + value):");
    for e in &q1_int_ev {
        println!("  {e}\n");
    }
    println!("FLOAT destination evidence (does the tag read 66?):");
    for e in &q1_float_ev {
        println!("  {e}\n");
    }
    if q1_float_ev.is_empty() {
        println!("  (no float-tagged remote set destination was attributed)\n");
    }
    println!("unresolvable chains:");
    for e in &q1_nochain_ev {
        println!("  {e}");
    }
    println!();

    // =========================================================================
    // Q2: placed-ref remotes
    // =========================================================================
    println!("=== Q2: placed-reference remotes (REFR/ACHR chain) ===\n");
    let mut q2_shown = 0usize;
    let mut q2_chains: std::collections::BTreeMap<String, usize> = Default::default();
    for (ui, d) in &all {
        let u = &units[*ui];
        for use_ in &d.uses {
            let Some(idx) = resolve(u, &use_.script_part) else {
                continue;
            };
            let Some(entry) = u.refs.get(idx as usize - 1) else {
                continue;
            };
            if entry.slsd_id.is_some() {
                continue;
            }
            if !is_placed_ref(recs.get(&entry.fid).map(|r| r.sig).unwrap_or([0; 4])) {
                continue;
            }
            let (steps, script_fid, _) = chain(entry.fid);
            let key = if script_fid.is_some() {
                if steps.contains("--NAME(base)--") {
                    "placed ref -> NAME(base) -> SCRV/SCRI -> SCPT"
                } else {
                    "placed ref -> SCRV/SCRI -> SCPT (script on the ref itself)"
                }
            } else {
                "placed ref -> NO SCRIPT"
            };
            *q2_chains.entry(key.to_string()).or_insert(0) += 1;
            if q2_shown < 8 {
                println!(
                    "--- {} (formID {:08X}) line {} ---",
                    u.edid,
                    u.form_id,
                    use_.line_no + 1
                );
                println!("    src: {}", use_.line);
                println!("    {}.{}  refIdx {}", use_.script_part, use_.var_part, idx);
                println!("    chain: {steps}");
                let conf: Vec<String> = d
                    .actuals
                    .iter()
                    .filter(|a| a.idx == idx)
                    .map(|a| {
                        format!(
                            "72 {} {:02X} {} [{}]",
                            a.idx,
                            a.tag,
                            a.id,
                            position(a.opcode, a.off)
                        )
                    })
                    .collect();
                if conf.is_empty() {
                    println!("    bytecode: no compound with refIdx {idx} in this unit");
                } else {
                    for c in &conf {
                        println!("    bytecode compound: {c}");
                    }
                }
                println!();
                q2_shown += 1;
            }
        }
    }
    println!("placed-ref chain outcomes (per source use):");
    let mut q2c: Vec<(String, usize)> = q2_chains.into_iter().collect();
    q2c.sort_by_key(|r| std::cmp::Reverse(r.1));
    for (k, c) in &q2c {
        println!("  {c:>6}  {k}");
    }
    println!();

    // =========================================================================
    // Q3: command argument
    // =========================================================================
    println!("=== Q3: command arguments ===\n");
    let mut by_op: std::collections::BTreeMap<String, usize> = Default::default();
    let mut showmessage = 0usize;
    let mut if_expr = 0usize;
    let mut begin_compound = 0usize;
    let mut set_dest = 0usize;
    let mut set_value = 0usize;
    let mut cmd_attributed = 0usize;
    let mut cmd_attributed_ev: Vec<String> = Vec::new();
    for a in &attributed {
        let key = format!("0x{:04X}", a.act.opcode);
        *by_op.entry(key).or_insert(0) += 1;
        match position(a.act.opcode, a.act.off) {
            "ShowMessage format value" => showmessage += 1,
            "expression operand (if/elseif)" => if_expr += 1,
            "Begin parameter" => begin_compound += 1,
            "set DESTINATION" => set_dest += 1,
            "set VALUE (RHS)" => set_value += 1,
            "command argument" => {
                cmd_attributed += 1;
                let u = &units[a.unit];
                if cmd_attributed_ev.len() < 20 {
                    cmd_attributed_ev.push(format!(
                        "{} (formID {:08X}) line {}: {}\n      72 {} {:02X} {} in op 0x{:04X} @{} off {}  chain: {}",
                        u.edid,
                        u.form_id,
                        a.use_.line_no + 1,
                        a.use_.line,
                        a.act.idx,
                        a.act.tag,
                        a.act.id,
                        a.act.opcode,
                        a.act.at,
                        a.act.off,
                        a.chain
                    ));
                }
            }
            _ => {}
        }
    }
    println!("ATTRIBUTED compounds by position (source use predicted the exact bytes):");
    let mut bo: Vec<(String, usize)> = by_op.into_iter().collect();
    bo.sort_by_key(|r| std::cmp::Reverse(r.1));
    for (k, c) in &bo {
        println!("  {c:>6}  opcode {k}");
    }
    println!(
        "positions: set DESTINATION {set_dest}; set VALUE {set_value}; ShowMessage format value {showmessage}; if/elseif expression {if_expr}; Begin parameter {begin_compound}; COMMAND ARGUMENT (non-ShowMessage) {cmd_attributed}"
    );
    println!("command-argument evidence:");
    if cmd_attributed_ev.is_empty() {
        println!("  NONE — no attributed compound sits inside a command call's arguments.\n");
    } else {
        for e in &cmd_attributed_ev {
            println!("  {e}\n");
        }
    }

    // Cross-check: ALL actual compounds inside command opcodes (attributed or
    // not), with the record chain of their refIdx. Adjacency noise included —
    // the attribution above is the discriminator.
    let mut cmd_all: Vec<(usize, Actual)> = Vec::new();
    for (ui, d) in &all {
        for a in &d.actuals {
            if a.opcode >= 0x1000 && a.opcode != 0x1059 {
                cmd_all.push((*ui, *a));
            }
        }
    }
    println!(
        "cross-check: {} actual compounds sit inside non-ShowMessage command opcodes (attributed or not; includes `72 <ref> 73 <var>` argument adjacency)\n",
        cmd_all.len()
    );
    let mut q3_shown = 0usize;
    for (ui, a) in &cmd_all {
        let u = &units[*ui];
        let chain_str = match u.refs.get(a.idx as usize - 1) {
            Some(e) if e.slsd_id.is_none() => chain(e.fid).0,
            Some(_) => format!("SCRV entry #{} (a declared ref var)", a.idx),
            None => format!("idx {} past the {}-entry list", a.idx, u.refs.len()),
        };
        println!(
            "{} (formID {:08X}) op 0x{:04X} @{} off {}: 72 {} {:02X} {}  refIdx {} -> {}",
            u.edid, u.form_id, a.opcode, a.at, a.off, a.idx, a.tag, a.id, a.idx, chain_str
        );
        let mut lines = Vec::new();
        for (li, line) in u.source.lines().enumerate() {
            let t = strip_comment(line).trim();
            let low = t.to_ascii_lowercase();
            if low.starts_with("set ")
                || low.starts_with("let ")
                || low.starts_with("if ")
                || low.starts_with("begin ")
                || low.starts_with("elseif ")
            {
                continue;
            }
            if t.split_whitespace().any(|w| {
                let w = w.trim_matches(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'));
                w.contains('.') && !w.starts_with('"')
            }) {
                lines.push(format!("line {}: {t}", li + 1));
            }
        }
        for l in lines.iter().take(3) {
            println!("    candidate src: {l}");
        }
        println!();
        q3_shown += 1;
        if q3_shown >= 25 {
            break;
        }
    }

    // =========================================================================
    // Q4: the 47 destination
    // =========================================================================
    println!("=== Q4: set destinations that write 47 <refIdx> ===\n");
    let mut q4_shown = 0usize;
    let mut q4_kinds: std::collections::BTreeMap<String, usize> = Default::default();
    let mut q4_ev: Vec<String> = Vec::new();
    for u in &units {
        for ins in read_instructions(&u.compiled) {
            if ins.opcode != 0x0015 || ins.args.len() < 4 || ins.args[0] != TAG_GLOBAL {
                continue;
            }
            let idx = u16::from_le_bytes([ins.args[1], ins.args[2]]);
            let named = name_idx(u, idx);
            let kind = if named.contains(" GLOB ") {
                "GLOB"
            } else if named.contains("SCRV") {
                "SCRV (ref var)"
            } else if named.contains("declared ref") {
                "declared ref"
            } else {
                "other"
            };
            *q4_kinds.entry(kind.to_string()).or_insert(0) += 1;
            if q4_shown < 40 {
                let target_name = match u.refs.get(idx as usize - 1) {
                    Some(e) if e.slsd_id.is_none() => {
                        recs.get(&e.fid).map(|r| r.edid.clone()).unwrap_or_default()
                    }
                    _ => format!("__idx{idx}"),
                };
                let mut src_lines = Vec::new();
                for (li, line) in u.source.lines().enumerate() {
                    let t = strip_comment(line).trim();
                    let low = t.to_ascii_lowercase();
                    if !(low.starts_with("set ") || low.starts_with("let ")) {
                        continue;
                    }
                    let Some((target, _)) = t[4..].split_once(" to ") else {
                        continue;
                    };
                    let target = target.trim().to_ascii_lowercase();
                    if target == target_name.to_ascii_lowercase()
                        || target.ends_with(&format!(".{}", target_name.to_ascii_lowercase()))
                    {
                        src_lines.push(format!("line {}: {}", li + 1, strip_comment(line).trim()));
                    }
                }
                let entry = format!(
                    "{} (formID {:08X})\n      Set @{}: {}\n      dest 47 idx {} -> {}\n      src: {}",
                    u.edid,
                    u.form_id,
                    ins.at,
                    hex_ins(&ins),
                    idx,
                    named,
                    if src_lines.is_empty() {
                        format!("(no set line names `{target_name}`)")
                    } else {
                        src_lines.join("\n      src: ")
                    }
                );
                if q4_ev.len() < 25 {
                    q4_ev.push(entry);
                }
                q4_shown += 1;
            }
        }
    }
    println!("47-destination Sets, what the idx names:");
    let mut q4k: Vec<(String, usize)> = q4_kinds.into_iter().collect();
    q4k.sort_by_key(|r| std::cmp::Reverse(r.1));
    for (k, c) in &q4k {
        println!("  {c:>6}  {k}");
    }
    println!("evidence:");
    for e in &q4_ev {
        println!("  {e}\n");
    }

    // Q4b: 47 (G) vs 5A (Z) standalone operands in if/elseif expressions.
    println!("=== Q4b: 47 (G/global) vs 5A (Z/ref) standalone operands in if/elseif ===\n");
    let mut g_count = 0usize;
    let mut z_count = 0usize;
    let mut g_shown = 0usize;
    for u in &units {
        for ins in read_instructions(&u.compiled) {
            if ins.opcode != 0x0016 && ins.opcode != 0x0018 {
                continue;
            }
            for i in 0..=(ins.args.len().saturating_sub(4)) {
                if ins.args[i] == 0x20 && ins.args[i + 1] == TAG_GLOBAL {
                    g_count += 1;
                    if g_shown < 12 {
                        let idx = u16::from_le_bytes([ins.args[i + 2], ins.args[i + 3]]);
                        let named = name_idx(u, idx);
                        println!(
                            "{} (formID {:08X}) op 0x{:04X} @{}: 20 47 {} = {}\n      instr: {}",
                            u.edid,
                            u.form_id,
                            ins.opcode,
                            ins.at,
                            idx,
                            named,
                            hex_ins(&ins)
                        );
                        for (li, line) in u.source.lines().enumerate() {
                            let t = strip_comment(line).trim();
                            let low = t.to_ascii_lowercase();
                            if low.starts_with("if ") || low.starts_with("elseif ") {
                                println!("      cond line {}: {t}", li + 1);
                            }
                        }
                        println!();
                        g_shown += 1;
                    }
                } else if ins.args[i] == 0x20 && ins.args[i + 1] == 0x5A {
                    z_count += 1;
                }
            }
        }
    }
    println!("if/elseif standalone operands: 47 (G/global): {g_count}; 5A (Z/ref): {z_count}\n");

    // =========================================================================
    // Original deliverable: first 30 units with attributed compounds
    // =========================================================================
    println!("=== attributed remote compounds, first 30 units ===\n");
    let mut per_unit: std::collections::BTreeMap<usize, Vec<&Attributed>> = Default::default();
    for a in &attributed {
        per_unit.entry(a.unit).or_default().push(a);
    }
    let mut shown = 0usize;
    let mut pos_counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for (ui, list) in &per_unit {
        if shown >= 30 {
            break;
        }
        let u = &units[*ui];
        println!("--- {} (formID {:08X}) ---", u.edid, u.form_id);
        println!(
            "    reference list ({} entries): {}",
            u.refs.len(),
            u.refs
                .iter()
                .map(|r| match r.slsd_id {
                    None => recs
                        .get(&r.fid)
                        .map(|x| x.edid.clone())
                        .unwrap_or_else(|| format!("{:08X}", r.fid)),
                    Some(id) => format!("SCRV(refvar:{id})"),
                })
                .collect::<Vec<_>>()
                .join(", ")
        );
        for a in list {
            let pos = position(a.act.opcode, a.act.off);
            *pos_counts.entry(pos).or_insert(0) += 1;
            println!("  line {}: {}", a.use_.line_no + 1, a.use_.line);
            println!(
                "    {}.{} -> 72 {} {:02X} {}   [{}]",
                a.use_.script_part, a.use_.var_part, a.act.idx, a.act.tag, a.act.id, pos
            );
            println!("    chain: {}", a.chain);
            println!(
                "    remote script: {} var {} id {} ({})",
                a.script_edid,
                a.var_name,
                a.var_id,
                if a.var_is_int { "int" } else { "float" }
            );
            let instructions = read_instructions(&u.compiled);
            if let Some(ins) = instructions.iter().find(|i| i.at == a.act.at) {
                println!("    instr: {}", hex_ins(ins));
            }
            println!();
        }
        shown += 1;
    }
    println!("units shown: {shown}");
    println!("attributed compounds by position:");
    let mut pc: Vec<(&str, usize)> = pos_counts.into_iter().collect();
    pc.sort_by_key(|r| std::cmp::Reverse(r.1));
    for (p, c) in &pc {
        println!("  {c:>6}  {p}");
    }
    println!();

    // Unattributed sample.
    println!("=== unattributed actual compounds (sample of 40) ===\n");
    for (ui, a) in &unattributed_sample {
        let u = &units[*ui];
        let chain_str = match u.refs.get(a.idx as usize - 1) {
            Some(e) if e.slsd_id.is_none() => chain(e.fid).0,
            Some(_) => format!("SCRV entry #{} (a declared ref var)", a.idx),
            None => format!("idx {} past the {}-entry list", a.idx, u.refs.len()),
        };
        println!(
            "{} op 0x{:04X} off {}: 72 {} {:02X} {}  refIdx {} -> {}  [{}]",
            u.edid,
            a.opcode,
            a.off,
            a.idx,
            a.tag,
            a.id,
            a.idx,
            chain_str,
            position(a.opcode, a.off)
        );
    }
    println!();

    // Begin parameters (original question c).
    println!("=== (original c) Begin parameters ===\n");
    let mut begin_dotted = 0usize;
    let mut begin_compound_units = 0usize;
    for u in &units {
        let mut dotted_begins: Vec<(usize, String)> = Vec::new();
        for (li, line) in u.source.lines().enumerate() {
            let t = strip_comment(line).trim();
            let low = t.to_ascii_lowercase();
            let Some(rest) = low.strip_prefix("begin ") else {
                continue;
            };
            let event_len = rest.split_whitespace().next().map(|w| w.len()).unwrap_or(0);
            let after = &t["begin ".len() + event_len..];
            let has_dot = after.split_whitespace().any(|w| {
                let w = w.trim_matches(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'));
                let parts: Vec<&str> = w.split('.').collect();
                parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty()
            });
            if has_dot {
                dotted_begins.push((li, t.to_string()));
            }
        }
        if dotted_begins.is_empty() {
            continue;
        }
        let begins: Vec<_> = read_instructions(&u.compiled)
            .into_iter()
            .filter(|i| i.opcode == 0x0010)
            .collect();
        let has_compound = begins.iter().any(|b| {
            b.args
                .windows(6)
                .any(|w| w[0] == TAG_REF && (w[3] == TAG_INT || w[3] == TAG_FLOAT))
        });
        if has_compound {
            begin_compound_units += 1;
        }
        println!("--- {} (formID {:08X}) ---", u.edid, u.form_id);
        for (li, line) in &dotted_begins {
            println!("  line {}: {line}", li + 1);
        }
        for b in &begins {
            println!("    Begin @{}: {}", b.at, hex_ins(b));
        }
        println!();
        begin_dotted += dotted_begins.len();
    }
    println!(
        "Begin lines with a dotted parameter: {begin_dotted}; units whose Begin args contain a 72/tag compound: {begin_compound_units}\n"
    );

    // Census: compounds by position, refIdx space, and the two index models.
    println!("=== census: all actual compounds ===\n");
    let mut pos_all: std::collections::BTreeMap<&str, usize> = Default::default();
    let mut tags = [0usize; 2];
    let mut total = 0usize;
    for u in &units {
        for ins in read_instructions(&u.compiled) {
            if ins.args.len() < 6 {
                continue;
            }
            for i in 0..=(ins.args.len() - 6) {
                if ins.args[i] != TAG_REF {
                    continue;
                }
                let tag = ins.args[i + 3];
                if tag != TAG_INT && tag != TAG_FLOAT {
                    continue;
                }
                let pos = position(ins.opcode, i);
                *pos_all.entry(pos).or_insert(0) += 1;
                if tag == TAG_INT {
                    tags[0] += 1;
                } else {
                    tags[1] += 1;
                }
                total += 1;
            }
        }
    }
    println!("total compounds corpus-wide: {total}");
    println!("by position:");
    let mut pa: Vec<(&str, usize)> = pos_all.into_iter().collect();
    pa.sort_by_key(|r| std::cmp::Reverse(r.1));
    for (p, c) in &pa {
        println!("  {c:>6}  {p}");
    }
    println!("tag split: int(73)={} float/ref(66)={}", tags[0], tags[1]);
    println!();

    // The two index models, head to head, over the attributed compounds:
    // the game's refIdx vs what each model predicts for the same source name.
    // - record-order: SCRO and SCRV entries counted together, in record
    //   order, 1-based (SCRV entries included where they sit).
    // - declared-first: every SCRV entry first (in record order), then every
    //   SCRO entry.
    let mut order_ok = 0usize;
    let mut decl_first_ok = 0usize;
    let mut both_ok = 0usize;
    let mut neither = 0usize;
    let mut name_missing = 0usize;
    for a in &attributed {
        let u = &units[a.unit];
        let game = a.act.idx;
        let name = &a.use_.script_part;
        // Record-order prediction: the position of the entry whose record EDID
        // is `name` (SCRV entries counted too).
        let order_idx = u
            .refs
            .iter()
            .position(|r| {
                r.slsd_id.is_none()
                    && recs
                        .get(&r.fid)
                        .map(|x| x.edid.eq_ignore_ascii_case(name))
                        .unwrap_or(false)
            })
            .map(|p| (p + 1) as u16);
        // Declared-first prediction: SCRV entries first, then SCRO entries.
        let scrv_count = u.refs.iter().filter(|r| r.slsd_id.is_some()).count();
        let mut scro_seen = 0usize;
        let mut df_idx = None;
        for r in &u.refs {
            match r.slsd_id {
                Some(_) => {}
                None => {
                    scro_seen += 1;
                    if recs
                        .get(&r.fid)
                        .map(|x| x.edid.eq_ignore_ascii_case(name))
                        .unwrap_or(false)
                    {
                        df_idx = Some((scrv_count + scro_seen) as u16);
                        break;
                    }
                }
            }
        }
        match (order_idx, df_idx) {
            (Some(o), Some(d)) => {
                if game == o && game == d {
                    both_ok += 1;
                } else if game == o {
                    order_ok += 1;
                } else if game == d {
                    decl_first_ok += 1;
                } else {
                    neither += 1;
                }
            }
            _ => name_missing += 1,
        }
    }
    println!("index models over attributed compounds (whose refIdx equals the game's):");
    println!("  record-order SCRO+SCRV model: {order_ok} (plus {both_ok} where both models agree)");
    println!("  declared-refs-first model:    {decl_first_ok}");
    println!("  neither model:                {neither}; name not in list: {name_missing}");
    println!();
}

impl RecInfo {
    fn sig_str(&self) -> String {
        String::from_utf8_lossy(&self.sig).into_owned()
    }
}
