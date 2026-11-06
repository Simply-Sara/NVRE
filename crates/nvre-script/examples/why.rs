//! One failing unit, explained: our bytes against the game's, instruction by
//! instruction, with the source statement each one came from.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example why -- 001541E3
//! NVRE_DATA=<Data> cargo run -p nvre-script --example why -- VNelsonBattlefieldSCRIPT
//! ```
//!
//! With no argument it reports every unit that does not reproduce, grouped by
//! the first instruction where the two streams part company — which is what
//! says which encoding rule is missing rather than which unit happens to sort
//! first.
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::{read_instructions, Instruction};
use nvre_script::opcodes::{command_at, Statement as StmtOp};

enum RefListEntry {
    Scro(u32),
    Scrv(u16),
}

struct Unit {
    fid: u32,
    edid: String,
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

    // The plugin's own masters, in load order — a DLC's scripts name forms that
    // live in the master, so compiling it alone resolves nothing.
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
            let Ok(bytes) = std::fs::read(std::path::Path::new(&data).join(&name)) else {
                eprintln!("warning: master {name} not found");
                continue;
            };
            if let Ok(p) = Plugin::parse_bytes(Bytes::from(bytes), name.clone()) {
                masters.push(p);
            }
        }
    }

    let mut edid_of: std::collections::HashMap<u32, String> = Default::default();
    let mut sig_of: std::collections::HashMap<u32, String> = Default::default();
    let mut scri: std::collections::HashMap<u32, u32> = Default::default();
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
    edid_of.insert(0x00000014, "Player".to_string());

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
        let mut ref_list: Vec<RefListEntry> = Vec::new();
        let mut scro_fids: Vec<u32> = Vec::new();
        for s in subs.iter() {
            if s.sig == *b"SCRO" && s.data.len() == 4 {
                let fid = u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]);
                scro_fids.push(fid);
                ref_list.push(RefListEntry::Scro(fid));
            } else if s.sig == *b"SCRV" && s.data.len() == 4 {
                ref_list.push(RefListEntry::Scrv(u16::from_le_bytes([
                    s.data[0], s.data[1],
                ])));
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
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        units.push(Unit {
            fid: rec.header.form_id,
            edid,
            source,
            expected: compiled,
            ref_list,
            scro_fids,
            vars,
        });
    });

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
    let remotes_for = |unit: &Unit| {
        let mut remotes: Vec<nvre_script::RemoteScript> = Vec::new();
        for scro_fid in &unit.scro_fids {
            let remote_name = edid_of
                .get(scro_fid)
                .cloned()
                .unwrap_or_else(|| format!("__form_{scro_fid:08X}"));
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
            if std::env::var("WHY_REMOTE_ORIGIN").is_ok() {
                eprintln!(
                    "remote {remote_name} -> script {script_edid} ({:08X}) vars={}",
                    script_fid,
                    table.len()
                );
            }
            remotes.push(nvre_script::RemoteScript {
                name: remote_name,
                vars: table,
            });
        }
        remotes
    };

    let wanted = std::env::args().nth(1);

    if let Some(wanted) = wanted {
        let w = wanted.trim_start_matches("0x");
        let as_fid = u32::from_str_radix(w, 16).ok();
        for unit in &units {
            let hit = unit.edid.eq_ignore_ascii_case(&wanted)
                || as_fid == Some(unit.fid)
                || format!("{:08X}", unit.fid).eq_ignore_ascii_case(w);
            if !hit {
                continue;
            }
            let refs = context_for(unit);
            let remotes = remotes_for(unit);
            println!("==== {:08X}  {} ====", unit.fid, unit.edid);
            println!(
                "refs: {:?}",
                refs.iter()
                    .map(|r| format!("{}:{:?}:{:?}", r.name, r.slot, r.kind))
                    .collect::<Vec<_>>()
            );
            println!(
                "vars: {:?}",
                unit.vars
                    .iter()
                    .map(|v| format!("{}({} {})", v.name, v.id, if v.is_int { "s" } else { "f" }))
                    .collect::<Vec<_>>()
            );
            if let Ok(name) = std::env::var("WHY_RESOLVE") {
                // `WHY_RESOLVE=<name>` also prints the remote tables, because a
                // dotted remote variable resolves through them: when several
                // names collapse to one id, the question is what the target
                // script's table actually holds.
                for r in &remotes {
                    if name == "*" {
                        println!("  remote {} vars={}", r.name, r.vars.len());
                        for v in &r.vars {
                            println!("    {} id={} int={}", v.name, v.id, v.is_int);
                        }
                        continue;
                    }
                    let hit = r.vars.iter().find(|v| v.name.eq_ignore_ascii_case(&name));
                    println!(
                        "  remote {} vars={} {}{}",
                        r.name,
                        r.vars.len(),
                        if hit.is_some() { "HAS " } else { "no  " },
                        name
                    );
                    if let Some(v) = hit {
                        println!("    {} id={} int={}", v.name, v.id, v.is_int);
                    }
                }
                for (i, r) in refs.iter().enumerate() {
                    if r.name.eq_ignore_ascii_case(&name) {
                        println!("  refs[{i}] = {:?} slot={:?}", r.name, r.slot);
                    }
                }
                println!("  resolve_ref_index({name}) = {:?}", {
                    let mut sc = nvre_script::parse::parse("");
                    sc.refs = refs.clone();
                    nvre_script::value::resolve_ref_index(&name, &sc)
                });
            }
            if std::env::var("WHY_SOURCE").is_ok() {
                for (i, line) in unit.source.lines().enumerate() {
                    println!("  {:>4}| {line}", i + 1);
                }
            }
            if std::env::var("WHY_PARSE").is_ok() {
                let script = nvre_script::parse::parse(&unit.source);
                for (i, s) in script.stmts.iter().enumerate() {
                    println!("  [{i:>3}] {}", describe(s));
                }
            }
            // `WHY_DUMP=1` prints the game's own instruction stream whole, and
            // `WHY_FIND=<EDID>` prints only the instructions whose arguments
            // carry that form's FormID — the two ways to read an encoding the
            // emitter has not produced yet.
            if std::env::var("WHY_DUMP").is_ok() {
                for (i, ins) in read_instructions(&unit.expected).iter().enumerate() {
                    println!("  game [{i}] {}", show(Some(ins)));
                }
            }
            if let Ok(name) = std::env::var("WHY_FIND") {
                let Some(fid) = edid_of
                    .iter()
                    .find(|(_, e)| e.eq_ignore_ascii_case(&name))
                    .map(|(f, _)| *f)
                else {
                    println!("no record named {name}");
                    return;
                };
                println!("{name} = {fid:08X}");
                let want = fid.to_le_bytes();
                let ins = read_instructions(&unit.expected);
                for (i, ins_i) in ins.iter().enumerate() {
                    if ins_i.args.windows(4).any(|w| w == want) {
                        let lo = i.saturating_sub(2);
                        let hi = (i + 3).min(ins.len());
                        for (j, ins_j) in ins.iter().enumerate().take(hi).skip(lo) {
                            println!("  game [{j}] {}", show(Some(ins_j)));
                        }
                        println!("  --");
                    }
                }
            }
            match nvre_script::compile_with_refs(&unit.source, &refs, &unit.vars, &remotes) {
                Err(e) => {
                    println!("compile error: {e}");
                    // The line the message names, or the line holding the
                    // backticked token it names.
                    let token = e
                        .to_string()
                        .split('`')
                        .nth(1)
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let context: usize = std::env::var("WHY_CONTEXT")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    let lines: Vec<&str> = unit.source.lines().collect();
                    for (i, line) in lines.iter().enumerate() {
                        if !token.is_empty() && line.contains(&token) {
                            let lo = i.saturating_sub(context);
                            let hi = (i + context + 1).min(lines.len());
                            for (j, ctx) in lines.iter().enumerate().take(hi).skip(lo) {
                                let mark = if j == i { ">" } else { " " };
                                println!("  {mark}line {:>4}: {}", j + 1, ctx.trim_end());
                            }
                        }
                    }
                }
                Ok(got) => {
                    let (ours, theirs) =
                        (read_instructions(&got), read_instructions(&unit.expected));
                    let script = nvre_script::parse::parse(&unit.source);
                    println!(
                        "bytes: got {} expected {}   instructions: got {} expected {}",
                        got.len(),
                        unit.expected.len(),
                        ours.len(),
                        theirs.len()
                    );
                    let mut shown = 0;
                    for i in 0..ours.len().max(theirs.len()) {
                        let a = ours.get(i);
                        let b = theirs.get(i);
                        let same = match (a, b) {
                            (Some(a), Some(b)) => {
                                a.opcode == b.opcode && normalized(a) == normalized(b)
                            }
                            _ => false,
                        };
                        if same {
                            continue;
                        }
                        println!("  source: {}", statement_at(&script, i));
                        println!("  [{i}] ours: {}", show(a));
                        println!("  [{i}] game: {}", show(b));
                        if shown >= 3 {
                            println!("  ...");
                            break;
                        }
                        shown += 1;
                    }
                }
            }
            println!();
        }
        return;
    }

    // No argument: every failing unit, grouped by where the streams differ.
    let mut groups: std::collections::BTreeMap<String, Vec<(u32, String)>> = Default::default();
    for unit in &units {
        let refs = context_for(unit);
        let remotes = remotes_for(unit);
        let why = match nvre_script::compile_with_refs(&unit.source, &refs, &unit.vars, &remotes) {
            Err(e) => format!("error: {e}"),
            Ok(got) if got == unit.expected => continue,
            Ok(got) => {
                let (ours, theirs) = (read_instructions(&got), read_instructions(&unit.expected));
                let script = nvre_script::parse::parse(&unit.source);
                let mut first = String::from("(no instruction difference)");
                for i in 0..ours.len().max(theirs.len()) {
                    let a = ours.get(i);
                    let b = theirs.get(i);
                    let same = match (a, b) {
                        (Some(a), Some(b)) => {
                            a.opcode == b.opcode && normalized(a) == normalized(b)
                        }
                        _ => false,
                    };
                    if !same {
                        first = format!(
                            "[{}] {} | game {} | at: {}",
                            i,
                            show(a),
                            show(b),
                            statement_at(&script, i)
                        );
                        break;
                    }
                }
                first
            }
        };
        groups
            .entry(why)
            .or_default()
            .push((unit.fid, unit.edid.clone()));
    }
    let mut rows: Vec<(&String, &Vec<(u32, String)>)> = groups.iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.len()));
    for (why, members) in rows {
        println!("{:>4}  {why}", members.len());
        for (fid, edid) in members.iter().take(6) {
            println!("        {fid:08X}  {edid}");
        }
    }
}

/// What a `Begin`'s argument bytes mean, for comparison purposes: the event
/// code first, then the block's own length. A body that is the wrong length
/// moves `blockLen` and nothing else at that instruction, so comparing it
/// verbatim reports "the block length differs" for every length bug and
/// hides the instruction inside the body that actually went wrong.
fn normalized(i: &Instruction) -> Vec<u8> {
    let mut a = i.args.clone();
    if i.opcode == 0x0010 && a.len() >= 6 {
        for b in a.iter_mut().skip(2).take(4) {
            *b = 0;
        }
    }
    a
}

/// How many instructions a statement emits, so a diverging instruction index
/// can be traced back to the source line that produced it.
fn instructions_of(s: &nvre_script::Stmt) -> usize {
    use nvre_script::Stmt;
    match s {
        Stmt::Decl(_) | Stmt::Unknown { .. } => 0,
        Stmt::Set { on, .. } | Stmt::Call { on, .. } => 1 + usize::from(on.is_some()),
        _ => 1,
    }
}

/// The source statement whose emitted instructions include `index`.
fn statement_at(script: &nvre_script::Script, index: usize) -> String {
    let mut at = 0usize;
    for s in &script.stmts {
        let n = instructions_of(s);
        if index < at + n {
            return describe(s);
        }
        at += n;
    }
    "<past the last statement>".to_string()
}

fn describe(s: &nvre_script::Stmt) -> String {
    use nvre_script::Stmt;
    match s {
        Stmt::ScriptName(n) => format!("ScriptName {n}"),
        Stmt::Begin { event, args } => format!("Begin {event} ({} arg tokens)", args.len()),
        Stmt::End => "End".to_string(),
        Stmt::Set { on, target, expr } => {
            format!("set {:?}{:?} to ({} value tokens)", on, target, expr.len())
        }
        Stmt::If(e) => format!("if ({} tokens)", e.len()),
        Stmt::Else => "else".to_string(),
        Stmt::ElseIf(e) => format!("elseif ({} tokens)", e.len()),
        Stmt::EndIf => "endif".to_string(),
        Stmt::Decl(k) => format!("declaration ({k:?})"),
        Stmt::Return => "return".to_string(),
        Stmt::Call { on, command, args } => {
            format!("call {on:?}.{command} ({} arg tokens)", args.len())
        }
        Stmt::Unknown { line, text } => format!("unread line {}: {text}", line + 1),
    }
}

fn show(i: Option<&Instruction>) -> String {
    match i {
        None => "<missing>".to_string(),
        Some(i) => {
            let name = if StmtOp::is_statement(i.opcode) {
                StmtOp::from_opcode(i.opcode)
                    .map(|s| s.keyword().to_string())
                    .unwrap_or_default()
            } else {
                command_at(i.opcode)
                    .map(|c| c.name.to_string())
                    .unwrap_or_default()
            };
            format!(
                "{:04X} {:14} len={} {:02X?}",
                i.opcode,
                name,
                i.args.len(),
                &i.args[..i.args.len().min(80)]
            )
        }
    }
}
