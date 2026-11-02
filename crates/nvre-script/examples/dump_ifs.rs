//! Probe: the encoding of an `if` whose condition names only declared vars.
//!
//! 251 units in the corpus have such a condition, and they are the ones the
//! compiler can emit without solving the reference blocker. This prints their
//! instruction streams so the `if`/`else`/`endif` shape can be read off the
//! game's own bytes.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example dump_ifs
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::{read_instructions, Instruction};

struct Unit {
    edid: String,
    source: String,
    compiled: Vec<u8>,
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
        if !compiled.is_empty() && !source.is_empty() {
            units.push(Unit {
                edid,
                source,
                compiled,
            });
        }
    });

    const KW: &[&str] = &[
        "scn",
        "scriptname",
        "begin",
        "end",
        "set",
        "if",
        "else",
        "elseif",
        "endif",
        "return",
        "short",
        "long",
        "int",
        "float",
        "ref",
        "to",
        "while",
        "and",
        "or",
        "not",
    ];

    // Units whose every `if`/`elseif` condition names only declared variables
    // and numbers — the ones emittable without the reference table.
    let mut usable: Vec<&Unit> = units
        .iter()
        .filter(|u| {
            let mut declared: Vec<String> = Vec::new();
            for l in u.source.lines() {
                let t = l.trim();
                let first = t.split_whitespace().next().unwrap_or("");
                if ["short", "long", "int", "float", "ref"]
                    .iter()
                    .any(|k| first.eq_ignore_ascii_case(k))
                {
                    if let Some(n) = t.split_whitespace().nth(1) {
                        declared.push(n.trim_end_matches(';').to_string());
                    }
                }
            }
            let mut has_if = false;
            for l in u.source.lines() {
                let t = l.trim();
                let first = t.split_whitespace().next().unwrap_or("");
                if first.eq_ignore_ascii_case("if") || first.eq_ignore_ascii_case("elseif") {
                    has_if = true;
                    let cond = t.splitn(2, char::is_whitespace).nth(1).unwrap_or("");
                    for w in cond.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                        if w.is_empty() || KW.iter().any(|k| w.eq_ignore_ascii_case(k)) {
                            continue;
                        }
                        if w.bytes().next().is_some_and(|b| b.is_ascii_digit()) {
                            continue;
                        }
                        if !declared.iter().any(|d| d.eq_ignore_ascii_case(w)) {
                            return false;
                        }
                    }
                }
            }
            has_if
        })
        .collect();
    usable.sort_by_key(|u| u.compiled.len());

    // Every `If` instruction's argument bytes, with the source line that
    // produced it, so the leading field can be resolved against the number of
    // tokens in the condition.
    println!("=== If arguments, with their source conditions ===");
    let mut shown = 0;
    for u in usable.iter() {
        if shown >= 14 {
            break;
        }
        // The unit's `if` lines, in order.
        let if_lines: Vec<String> = u
            .source
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.split_whitespace()
                    .next()
                    .is_some_and(|w| w.eq_ignore_ascii_case("if"))
            })
            .map(|l| l.trim().to_string())
            .collect();
        let all = read_instructions(&u.compiled);
        let if_ins: Vec<&Instruction> = all.iter().filter(|i| i.opcode == 0x16).collect();
        if if_lines.len() != if_ins.len() || if_lines.is_empty() {
            continue;
        }
        for (line, ins) in if_lines.iter().zip(if_ins.iter()) {
            println!("  {:<52} args {:02X?}", line, ins.args);
            shown += 1;
            if shown >= 14 {
                break;
            }
        }
    }

    // The leading field against the number of `if` statements in the unit, so
    // the correlation can be tested rather than assumed.
    println!();
    println!("=== leading field vs the unit's if-count ===");
    let mut rows: Vec<(u16, usize, usize, String)> = Vec::new();
    for u in usable.iter() {
        let if_count = u
            .source
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.split_whitespace()
                    .next()
                    .is_some_and(|w| w.eq_ignore_ascii_case("if"))
            })
            .count();
        let all = read_instructions(&u.compiled);
        let if_ins: Vec<&Instruction> = all.iter().filter(|i| i.opcode == 0x16).collect();
        for ins in &if_ins {
            let lead = u16::from_le_bytes([ins.args[0], ins.args[1]]);
            rows.push((lead, if_count, if_ins.len(), u.edid.clone()));
        }
    }
    // Group by the leading value and show what if-counts it appears with.
    let mut by_lead: std::collections::BTreeMap<u16, std::collections::BTreeMap<usize, usize>> =
        std::collections::BTreeMap::new();
    for (lead, count, _, _) in &rows {
        *by_lead.entry(*lead).or_default().entry(*count).or_insert(0) += 1;
    }
    for (lead, counts) in &by_lead {
        let c: Vec<String> = counts.iter().map(|(n, k)| format!("{n}ifs:{k}")).collect();
        println!("  lead={lead}  {}", c.join("  "));
    }

    // Is the leading field maybe the count of tokens in the expression, or the
    // count of conditions ORed/ANDed? Check it against the expression's own
    // token count.
    println!();
    println!("=== lead vs the expression's own token count ===");
    let mut by_pair: std::collections::BTreeMap<(u16, usize), usize> =
        std::collections::BTreeMap::new();
    for u in usable.iter() {
        let all = read_instructions(&u.compiled);
        for ins in all.iter().filter(|i| i.opcode == 0x16) {
            if ins.args.len() < 4 {
                continue;
            }
            let lead = u16::from_le_bytes([ins.args[0], ins.args[1]]);
            // Count the 0x20-tagged tokens in the expression.
            let expr = &ins.args[4..];
            let tokens = expr.iter().filter(|b| **b == 0x20).count();
            *by_pair.entry((lead, tokens)).or_insert(0) += 1;
        }
    }
    for ((lead, tokens), count) in by_pair.iter().take(30) {
        println!("  lead={lead:<3} exprTokens={tokens:<3} x{count}");
    }

    // Last hypothesis: is the leading field constant per script? If every
    // `If` in one unit carries the same value, it is a per-unit counter of
    // some kind and still not derivable from the source.
    println!();
    println!("=== is the lead constant within a unit? ===");
    let mut constant = 0usize;
    let mut varying = 0usize;
    let mut examples: Vec<String> = Vec::new();
    for u in usable.iter() {
        let all = read_instructions(&u.compiled);
        let leads: Vec<u16> = all
            .iter()
            .filter(|i| i.opcode == 0x16 && i.args.len() >= 2)
            .map(|i| u16::from_le_bytes([i.args[0], i.args[1]]))
            .collect();
        if leads.len() < 2 {
            continue;
        }
        if leads.windows(2).all(|w| w[0] == w[1]) {
            constant += 1;
        } else {
            varying += 1;
            if examples.len() < 6 {
                examples.push(format!("{}: {:?}", u.edid, leads));
            }
        }
    }
    println!("  units where every If has the same lead: {constant}");
    println!("  units where the lead varies:             {varying}");
    for e in &examples {
        println!("    {e}");
    }

    println!();
    println!("=== the 6 smallest units with a reference-free `if` ===");
    for u in usable.iter().take(6) {
        println!();
        println!("--- {} ({} bytes) ---", u.edid, u.compiled.len());
        for line in u.source.lines() {
            if !line.trim().is_empty() {
                println!("  | {line}");
            }
        }
        println!("  instructions:");
        for ins in read_instructions(&u.compiled) {
            let name = nvre_script::opcodes::Statement::from_opcode(ins.opcode)
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|| "command".to_string());
            println!(
                "    @{:<4} 0x{:04X} {:<12} args[{}] = {:02X?}",
                ins.at,
                ins.opcode,
                name,
                ins.args.len(),
                ins.args
            );
        }
    }
}
