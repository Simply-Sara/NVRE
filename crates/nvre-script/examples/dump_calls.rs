//! Probe: how the game encodes a command call.
//!
//! A command call is the most common statement in the corpus, and its
//! encoding is the one that unlocks `if` as well — an `if` condition is
//! usually a command call returning a bool. This finds the simplest units
//! that contain one and prints their instruction streams, so the shape can be
//! read off the game's own bytes rather than reasoned from a spec.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example dump_calls
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

    // The units with the fewest command-call-looking lines, so the simplest
    // ones come first: one call and nothing else to confuse the reading.
    let mut with_calls: Vec<&Unit> = units
        .iter()
        .filter(|u| {
            u.source
                .lines()
                .filter(|l| {
                    let t = l.trim();
                    // A line that is neither a keyword statement nor a
                    // declaration and is not blank.
                    !t.is_empty()
                        && !t.starts_with(';')
                        && !t.split_whitespace().next().is_some_and(|w| {
                            w.eq_ignore_ascii_case("scn")
                                || w.eq_ignore_ascii_case("scriptname")
                                || w.eq_ignore_ascii_case("begin")
                                || w.eq_ignore_ascii_case("end")
                                || w.eq_ignore_ascii_case("set")
                                || w.eq_ignore_ascii_case("if")
                                || w.eq_ignore_ascii_case("else")
                                || w.eq_ignore_ascii_case("elseif")
                                || w.eq_ignore_ascii_case("endif")
                                || w.eq_ignore_ascii_case("return")
                                || w.eq_ignore_ascii_case("short")
                                || w.eq_ignore_ascii_case("long")
                                || w.eq_ignore_ascii_case("int")
                                || w.eq_ignore_ascii_case("float")
                                || w.eq_ignore_ascii_case("ref")
                        })
                })
                .count()
                == 1
        })
        .collect();
    with_calls.sort_by_key(|u| u.compiled.len());

    // Calls with arguments, so the argument encoding can be read.
    //
    // The source line is found by counting: a unit's command calls appear in
    // source order, and so do its call instructions, so the nth call line
    // goes with the nth call instruction. A unit whose two counts differ is
    // skipped rather than mispaired.
    println!("=== the smallest calls that have arguments ===");
    let mut with_args: Vec<(usize, String, Vec<u8>, String)> = Vec::new();
    for u in &units {
        let call_lines: Vec<String> = u
            .source
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty()
                    && !t.starts_with(';')
                    && !t.split_whitespace().next().is_some_and(|w| {
                        w.eq_ignore_ascii_case("scn")
                            || w.eq_ignore_ascii_case("scriptname")
                            || w.eq_ignore_ascii_case("begin")
                            || w.eq_ignore_ascii_case("end")
                            || w.eq_ignore_ascii_case("set")
                            || w.eq_ignore_ascii_case("if")
                            || w.eq_ignore_ascii_case("else")
                            || w.eq_ignore_ascii_case("elseif")
                            || w.eq_ignore_ascii_case("endif")
                            || w.eq_ignore_ascii_case("return")
                            || w.eq_ignore_ascii_case("short")
                            || w.eq_ignore_ascii_case("long")
                            || w.eq_ignore_ascii_case("int")
                            || w.eq_ignore_ascii_case("float")
                            || w.eq_ignore_ascii_case("ref")
                    })
            })
            .map(|l| l.trim().to_string())
            .collect();
        let all_ins = read_instructions(&u.compiled);
        let call_ins: Vec<&Instruction> = all_ins.iter().filter(|i| i.opcode >= 0x1000).collect();
        if call_lines.len() != call_ins.len() {
            continue;
        }
        for (line, ins) in call_lines.iter().zip(call_ins.iter()) {
            if !ins.args.is_empty() {
                with_args.push((
                    ins.args.len(),
                    u.edid.clone(),
                    ins.args.clone(),
                    line.clone(),
                ));
            }
        }
    }
    with_args.sort_by_key(|(n, ..)| *n);
    println!("--- with more than the implicit 2 bytes ---");
    for (n, edid, args, call) in with_args.iter().filter(|(n, ..)| *n > 2).take(14) {
        println!("  {n} bytes  {edid:<34} {call}");
        println!("      {args:02X?}");
    }
    // Calls whose source spells a number as an argument, so the encoding of a
    // numeric argument can be read against the number it came from.
    println!("--- calls with a number spelled in the source ---");
    let mut numbered: Vec<&(usize, String, Vec<u8>, String)> = with_args
        .iter()
        .filter(|(.., c)| {
            c.split_whitespace().skip(1).any(|w| {
                w.bytes().next().is_some_and(|b| b.is_ascii_digit())
                    && w.bytes().all(|b| b.is_ascii_digit())
            })
        })
        .collect();
    numbered.sort_by_key(|(n, ..)| *n);
    for (n, edid, args, call) in numbered.iter().take(20) {
        println!("  {n} bytes  {edid:<32} {call}");
        println!("      {args:02X?}");
    }

    // Which tag bytes open a command's argument, and how many bytes each
    // takes. The tag set is measured rather than assumed, and a tag that
    // appears once is still recorded.
    // Each command opcode's argLen distribution, paired with the source line
    // that uses it. This resolves the `ResetAI` (argLen 0) versus `activate`
    // (argLen 2) question: does a zero-argument command write nothing, or a
    // count of zero?
    println!("=== per-opcode argLen, with a source line ===");
    let mut by_op: std::collections::BTreeMap<u16, (Vec<usize>, String, String)> =
        std::collections::BTreeMap::new();
    for u in &units {
        let call_lines: Vec<String> = u
            .source
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty()
                    && !t.starts_with(';')
                    && !t.split_whitespace().next().is_some_and(|w| {
                        w.eq_ignore_ascii_case("scn")
                            || w.eq_ignore_ascii_case("scriptname")
                            || w.eq_ignore_ascii_case("begin")
                            || w.eq_ignore_ascii_case("end")
                            || w.eq_ignore_ascii_case("set")
                            || w.eq_ignore_ascii_case("if")
                            || w.eq_ignore_ascii_case("else")
                            || w.eq_ignore_ascii_case("elseif")
                            || w.eq_ignore_ascii_case("endif")
                            || w.eq_ignore_ascii_case("return")
                            || w.eq_ignore_ascii_case("short")
                            || w.eq_ignore_ascii_case("long")
                            || w.eq_ignore_ascii_case("int")
                            || w.eq_ignore_ascii_case("float")
                            || w.eq_ignore_ascii_case("ref")
                    })
            })
            .map(|l| l.trim().to_string())
            .collect();
        let all_ins = read_instructions(&u.compiled);
        let call_ins: Vec<&Instruction> = all_ins.iter().filter(|i| i.opcode >= 0x1000).collect();
        if call_lines.len() != call_ins.len() {
            continue;
        }
        for (line, ins) in call_lines.iter().zip(call_ins.iter()) {
            let e = by_op
                .entry(ins.opcode)
                .or_insert((Vec::new(), String::new(), String::new()));
            if !e.1.contains(line.as_str()) && e.1.len() < 200 {
                if !e.1.is_empty() {
                    e.1.push('|');
                }
                e.1.push_str(line);
            }
            e.2 = u.edid.clone();
            if !e.0.contains(&ins.args.len()) {
                e.0.push(ins.args.len());
            }
        }
    }
    let mut zero: Vec<(u16, String, String)> = Vec::new();
    let mut two_only: Vec<(u16, String, String)> = Vec::new();
    for (op, (lens, lines, edid)) in &by_op {
        let mut lens = lens.clone();
        lens.sort();
        if lens == [0] {
            zero.push((*op, lines.clone(), edid.clone()));
        } else if lens.first() == Some(&2) {
            two_only.push((*op, lines.clone(), edid.clone()));
        }
    }
    println!("--- argLen is ALWAYS 0 (no declared parameters) ---");
    for (op, lines, edid) in &zero {
        println!("  0x{op:04X}  {edid:<28} {lines}");
    }
    println!("--- argLen includes 2 (declares parameters) ---");
    for (op, lines, edid) in two_only.iter().take(20) {
        println!("  0x{op:04X}  {edid:<28} {lines}");
    }

    // A named command's exact bytes, so a specific encoding can be measured.
    for want in ["vLegionaryFactionRewardNPCScript"] {
        if let Some(u) = units.iter().find(|u| u.edid.eq_ignore_ascii_case(want)) {
            println!();
            println!("--- {want} : {} bytes ---", u.compiled.len());
            for line in u.source.lines() {
                if !line.trim().is_empty() {
                    println!("  | {line}");
                }
            }
            println!("  full: {:02X?}", u.compiled);
            for ins in read_instructions(&u.compiled) {
                println!(
                    "    @{:<4} 0x{:04X} args[{}] = {:02X?}",
                    ins.at,
                    ins.opcode,
                    ins.args.len(),
                    ins.args
                );
            }
        }
    }

    println!("=== command argument tags ===");
    let mut tags: std::collections::BTreeMap<u8, Vec<usize>> = std::collections::BTreeMap::new();
    for (_, _, args, _) in &with_args {
        // Skip the argCount, then read tag + payload until the bytes run out.
        if args.len() < 2 {
            continue;
        }
        let mut i = 2usize;
        while i < args.len() {
            let tag = args[i];
            let rest = &args[i + 1..];
            // A `n` tag is followed by four bytes; a `r`/`s`/`f` by two.
            let width = match tag {
                0x6E => 4,
                0x72 | 0x73 | 0x66 => 2,
                _ => rest.len().min(4),
            };
            tags.entry(tag).or_default().push(width);
            i += 1 + width;
        }
    }
    for (tag, widths) in &tags {
        let name = match tag {
            0x6E => "'n' integer",
            0x72 => "'r' reference",
            0x73 => "'s' script variable",
            0x66 => "'f' float/ref variable",
            _ => "?",
        };
        let distinct: std::collections::BTreeSet<&usize> = widths.iter().collect();
        println!(
            "  0x{tag:02X} {name:<24} {} uses, widths {distinct:?}",
            widths.len()
        );
    }

    println!("--- with a dotted reference (Ref.Command) ---");
    for (n, edid, args, call) in with_args.iter().filter(|(.., c)| c.contains('.')).take(10) {
        println!("  {n} bytes  {edid:<34} {call}");
        println!("      {args:02X?}");
    }

    println!("--- the smallest ---");
    for (n, edid, args, call) in with_args.iter().take(8) {
        println!("  {n} bytes  {edid:<34} {call}");
        println!("      {args:02X?}");
    }

    println!();
    println!("=== the 8 smallest units with exactly one command call ===");
    for u in with_calls.iter().take(8) {
        println!();
        println!("--- {} ---", u.edid);
        println!("source:");
        for line in u.source.lines() {
            if !line.trim().is_empty() {
                println!("  | {line}");
            }
        }
        println!("instructions:");
        for ins in read_instructions(&u.compiled) {
            println!(
                "  @{:<4} 0x{:04X}  args[{}] = {:02X?}",
                ins.at,
                ins.opcode,
                ins.args.len(),
                ins.args
            );
        }
    }
}
