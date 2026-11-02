//! Probe: the obscript command names the shipped scripts call, and the opcodes
//! their compiled bytes use.
//!
//! A compiler needs a name→opcode table. The game's own registry is inside
//! `GECK.exe`, but the opcodes are not stored there as text — so the table is
//! built the other way round: every `SCPT` carries both its source (`SCTX`)
//! and its compiled bytes (`SCDA`), so the opcodes the bytes use and the
//! command names the source spells are two views of the same call sites.
//!
//! This probe prints the two sets so they can be lined up: the distinct
//! command opcodes in the corpus, and the distinct command names in it. The
//! pairing — which name compiles to which opcode — is what the compiler's
//! table needs, and it is derived by matching a name's call site in the source
//! against the opcode at the same position in the bytes.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example obscript_names
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use std::collections::BTreeSet;

fn u16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// The opcodes a compiled body uses, in the order it uses them.
fn opcodes_in_order(bytes: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 4 <= bytes.len() {
        let op = u16(bytes, i);
        let len = u16(bytes, i + 2) as usize;
        out.push(op);
        i = (i + 4 + len).min(bytes.len());
    }
    out
}

/// The command names a source spells, in order.
///
/// A command call is a line whose first word is either a bare command name or
/// a reference followed by `.` and the command name. Keywords and control-flow
/// words are not commands, so they are skipped.
fn names_in_order(source: &str) -> Vec<String> {
    const NOT_COMMANDS: &[&str] = &[
        "scn",
        "scriptname",
        "begin",
        "end",
        "if",
        "else",
        "elseif",
        "endif",
        "set",
        "let",
        "to",
        "while",
        "foreach",
        "loop",
        "return",
        "call",
        "continue",
        "break",
        "short",
        "long",
        "float",
        "ref",
        "reference",
        "eval",
        "function",
        "label",
        "goto",
    ];
    let mut out = Vec::new();
    for line in source.lines() {
        // A comment is not code.
        let code = match line.find(';') {
            Some(at) => &line[..at],
            None => line,
        };
        let mut words = code.split_whitespace();
        let Some(first) = words.next() else { continue };
        let lower = first.to_ascii_lowercase();
        if NOT_COMMANDS.contains(&lower.as_str()) {
            continue;
        }
        // `Ref.Command args` — the reference and the command are one word or
        // two, depending on whether the writer put a space after the dot.
        if let Some(dot) = first.find('.') {
            let (_, rest) = first.split_at(dot);
            let rest = rest.trim_start_matches('.');
            if !rest.is_empty() {
                out.push(rest.to_string());
                continue;
            }
            // The dot ended the word: the command is the next one.
            if let Some(next) = words.next() {
                out.push(next.to_string());
            }
            continue;
        }
        out.push(first.to_string());
    }
    out
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

    let mut units: Vec<(u32, Vec<u8>, String)> = Vec::new();
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
            units.push((rec.header.form_id, compiled, source));
        }
    });
    println!("{} units with both source and bytes", units.len());

    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut ops: BTreeSet<u16> = BTreeSet::new();
    let mut pairs: std::collections::BTreeMap<(String, u16), usize> =
        std::collections::BTreeMap::new();

    for (_, compiled, source) in &units {
        let ordered_ops = opcodes_in_order(compiled);
        let ordered_names = names_in_order(source);
        for n in &ordered_names {
            names.insert(n.to_ascii_lowercase());
        }
        for op in &ordered_ops {
            if *op >= 0x1000 {
                ops.insert(*op);
            }
        }
        // The pairing, per unit. The statement stream and the source's
        // statement lines run in the same order, so the nth command name on
        // the nth command opcode is right wherever the two are aligned — which
        // is most of the time, because a command call at statement level emits
        // exactly one opcode. The votes accumulate across the corpus, so the
        // noise from a misaligned unit is outvoted by the units that agree.
        let cmd_ops: Vec<u16> = ordered_ops.into_iter().filter(|o| *o >= 0x1000).collect();
        let cmd_names: Vec<String> = ordered_names
            .into_iter()
            .map(|n| n.to_ascii_lowercase())
            .collect();
        for (n, op) in cmd_names.iter().zip(cmd_ops.iter()) {
            *pairs.entry((n.clone(), *op)).or_default() += 1;
        }
    }

    println!("{} distinct command names in the source", names.len());
    println!("{} distinct command opcodes in the bytes", ops.len());
    println!();

    // The winner per name: the opcode the corpus votes for most often. A name
    // whose top vote is a clear majority is settled; the rest are listed with
    // their runners-up so the table can say which entries are uncertain
    // rather than presenting a guess as a fact.
    let mut by_name: std::collections::BTreeMap<String, Vec<(u16, usize)>> =
        std::collections::BTreeMap::new();
    for ((n, op), count) in &pairs {
        by_name.entry(n.clone()).or_default().push((*op, *count));
    }
    let mut settled = 0usize;
    let mut unsure = 0usize;
    println!("name -> opcode, by corpus vote:");
    for (n, mut votes) in by_name {
        votes.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        let (top_op, top) = votes[0];
        let total: usize = votes.iter().map(|(_, c)| *c).sum();
        let share = top as f64 / total as f64;
        if share >= 0.8 && top >= 3 {
            settled += 1;
            println!("  {n:32} 0x{top_op:04X}   {top}/{total}");
        } else {
            unsure += 1;
            let rest: Vec<String> = votes
                .iter()
                .take(4)
                .map(|(o, c)| format!("0x{o:04X}({c})"))
                .collect();
            println!("  ? {n:31} {}", rest.join(" "));
        }
    }
    println!();
    println!("{settled} names settled by a clear majority, {unsure} uncertain");
}
