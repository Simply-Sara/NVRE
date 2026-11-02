//! Probe: the first instructions of the simplest scripts in the corpus.
//!
//! The emitter is built one statement at a time, and each one needs to know
//! exactly what the game wrote for it. This prints the instruction stream of
//! the smallest scripts — the ones with a `ScriptName` and little else — so
//! the encoding of each statement can be read off the game's own bytes rather
//! than guessed from a spec.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example dump_units
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;

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

    struct Unit {
        fid: u32,
        edid: String,
        source: String,
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
                fid: rec.header.form_id,
                edid,
                source,
                compiled,
            });
        }
    });

    // How many lines of source each unit has, so the simplest ones come first.
    // Smallest by compiled size, so the first ones have real statements.
    units.sort_by_key(|u| u.compiled.len());

    println!("=== the 8 smallest compiled bodies ===");
    for u in units.iter().filter(|u| u.compiled.len() > 8).take(8) {
        println!();
        println!("--- {} ({:08X}) ---", u.edid, u.fid);
        println!("source:");
        for line in u.source.lines() {
            println!("  | {line}");
        }
        println!("instructions ({} bytes):", u.compiled.len());
        for ins in read_instructions(&u.compiled) {
            let stmt = nvre_script::opcodes::Statement::from_opcode(ins.opcode);
            let what = match stmt {
                Some(s) => format!("{s:?}"),
                None => "command".to_string(),
            };
            println!(
                "  @{:<4} 0x{:04X} {:<22} args[{}] = {:02X?}",
                ins.at,
                ins.opcode,
                what,
                ins.args.len(),
                ins.args
            );
        }
    }

    // A parameterless Begin's argument length, across the whole corpus. Two
    // readings of the frame are possible and the corpus decides between them:
    // 6 bytes means no parameter count at all, 8 means a count of zero is
    // always written.
    let mut empty_arg_lens: std::collections::BTreeMap<usize, usize> =
        std::collections::BTreeMap::new();
    let mut with_param: Vec<(String, Vec<u8>)> = Vec::new();
    for u in &units {
        for ins in read_instructions(&u.compiled) {
            if ins.opcode != 0x10 || ins.args.len() < 6 {
                continue;
            }
            // A Begin with a parameter writes one after the block length.
            if ins.args.len() > 8 {
                if with_param.len() < 5 {
                    with_param.push((u.edid.clone(), ins.args.clone()));
                }
            } else {
                *empty_arg_lens.entry(ins.args.len()).or_insert(0) += 1;
            }
        }
    }
    println!();
    println!("=== parameterless Begin argument lengths ===");
    for (len, count) in &empty_arg_lens {
        println!("  {len} bytes: {count} units");
    }
    println!("=== Begins with parameters (the tail after blockLen) ===");
    for (edid, args) in &with_param {
        println!("  {edid:<28} {:02X?}", args);
    }

    // What separates a 6-byte parameterless Begin from an 8-byte one. Both
    // forms exist in the corpus, so something decides which — the event code,
    // the script type, or something else. Print the event code of each.
    let mut six: Vec<u16> = Vec::new();
    let mut eight: Vec<u16> = Vec::new();
    let mut six_units: Vec<&str> = Vec::new();
    let mut eight_units: Vec<&str> = Vec::new();
    for u in &units {
        for ins in read_instructions(&u.compiled) {
            if ins.opcode != 0x10 || ins.args.len() < 6 {
                continue;
            }
            let event = u16::from_le_bytes([ins.args[0], ins.args[1]]);
            if ins.args.len() == 6 {
                six.push(event);
                if six_units.len() < 8 {
                    six_units.push(&u.edid);
                }
            } else if ins.args.len() == 8 {
                eight.push(event);
                if eight_units.len() < 8 {
                    eight_units.push(&u.edid);
                }
            }
        }
    }
    let events_of = |v: &[u16]| -> String {
        let mut c: std::collections::BTreeMap<u16, usize> = std::collections::BTreeMap::new();
        for e in v {
            *c.entry(*e).or_insert(0) += 1;
        }
        c.iter()
            .map(|(e, n)| format!("0x{e:04X}:{n}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    // The name the corpus spells for each event code. This is the authority:
    // the pairing is positional, so the nth source `Begin` is the nth bytecode
    // `Begin`, and the spelling is the writer's own.
    println!();
    println!("=== the name the corpus spells for each event code ===");
    let mut names_by_code: std::collections::BTreeMap<
        u16,
        std::collections::BTreeMap<String, usize>,
    > = std::collections::BTreeMap::new();
    let mut skipped = 0usize;
    for u in &units {
        let src_names: Vec<&str> = u
            .source
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                let first = words.next()?;
                if !first.eq_ignore_ascii_case("begin") {
                    return None;
                }
                words.next()
            })
            .collect();
        let codes: Vec<u16> = read_instructions(&u.compiled)
            .into_iter()
            .filter(|i| i.opcode == 0x10 && i.args.len() >= 6)
            .map(|i| u16::from_le_bytes([i.args[0], i.args[1]]))
            .collect();
        if src_names.len() != codes.len() {
            skipped += 1;
            continue;
        }
        for (name, code) in src_names.iter().zip(codes.iter()) {
            *names_by_code
                .entry(*code)
                .or_default()
                .entry((*name).to_string())
                .or_default() += 1;
        }
    }
    for (code, names) in &names_by_code {
        let top: Vec<String> = {
            let mut v: Vec<(&String, &usize)> = names.iter().collect();
            v.sort_by(|a, b| b.1.cmp(a.1));
            v.iter().take(3).map(|(n, c)| format!("{n}({c})")).collect()
        };
        println!("  0x{code:04X}  {}", top.join(", "));
    }
    println!("({skipped} units skipped)");

    println!();
    println!("=== 6-byte vs 8-byte parameterless Begins, by event ===");
    println!("  6 bytes: {}", events_of(&six));
    println!("  8 bytes: {}", events_of(&eight));
    println!("  (6-byte units: {six_units:?})");
    println!("  (8-byte units: {eight_units:?})");

    // A named unit's bytes, so a specific block can be measured exactly.
    for want in ["NCRCFScramblerScript", "vHDNCROutsideOliverScript"] {
        if let Some(u) = units.iter().find(|u| u.edid.eq_ignore_ascii_case(want)) {
            println!();
            println!(
                "--- {} ({:08X}) : {} bytes ---",
                u.edid,
                u.fid,
                u.compiled.len()
            );
            for line in u.source.lines() {
                if !line.trim().is_empty() {
                    println!("  | {line}");
                }
            }
            println!("  full body: {:02X?}", u.compiled);
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

    // Every Begin's argument shape across the corpus: how many arguments the
    // event takes, and what the block length is. This is the table the emitter
    // needs, and it is the game's own answer rather than a spec's.
    // One unit with several Begins, so the positional pairing can be checked by
    // eye before it is trusted over the whole corpus.
    println!();
    println!("=== a unit with several Begins ===");
    for u in units.iter() {
        let begins: Vec<&str> = u
            .source
            .lines()
            .filter(|l| {
                l.split_whitespace()
                    .next()
                    .is_some_and(|w| w.eq_ignore_ascii_case("begin"))
            })
            .collect();
        if begins.len() >= 3 {
            println!("--- {} ({:08X}) ---", u.edid, u.fid);
            for (n, b) in begins.iter().enumerate() {
                println!("  begin {n}: {}", b.trim());
            }
            for ins in read_instructions(&u.compiled) {
                if ins.opcode == 0x10 && ins.args.len() >= 4 {
                    let code = u16::from_le_bytes([ins.args[2], ins.args[3]]);
                    println!("  code 0x{code:04X}  args {:02X?}", ins.args);
                }
            }
            break;
        }
    }

    // Pair each Begin's event code with the event name its own source spells,
    // positionally: the unit's Begins in source order and its Begins in
    // bytecode order are the same Begins, so the nth name goes with the nth
    // code. A unit with a different number of the two is skipped rather than
    // mispaired — it would only contribute noise.
    println!();
    println!("=== event code <- the name the source spells ===");
    let mut by_code: std::collections::BTreeMap<u16, std::collections::BTreeMap<String, usize>> =
        std::collections::BTreeMap::new();
    let mut skipped = 0usize;
    for u in &units {
        let names: Vec<&str> = u
            .source
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                let first = words.next()?;
                if !first.eq_ignore_ascii_case("begin") {
                    return None;
                }
                words.next()
            })
            .collect();
        let codes: Vec<u16> = read_instructions(&u.compiled)
            .into_iter()
            .filter(|i| i.opcode == 0x10 && i.args.len() >= 4)
            .map(|i| u16::from_le_bytes([i.args[0], i.args[1]]))
            .collect();
        if names.len() != codes.len() {
            skipped += 1;
            continue;
        }
        for (name, code) in names.iter().zip(codes.iter()) {
            *by_code
                .entry(*code)
                .or_default()
                .entry((*name).to_string())
                .or_default() += 1;
        }
    }
    for (code, names) in &by_code {
        let names: Vec<String> = names.iter().map(|(n, c)| format!("{n}({c})")).collect();
        let mine = nvre_script::opcodes::Event::from_code(*code)
            .map(|e| e.name())
            .unwrap_or("?");
        println!(
            "  0x{code:04X}  NVRE says {mine:<24} corpus says {}",
            names.join(", ")
        );
    }
    println!("({skipped} units skipped: their Begin count and their source's differ)");

    println!();
    println!("=== Begin argument shapes across the corpus ===");
    let mut shapes: std::collections::BTreeMap<u16, Vec<usize>> = std::collections::BTreeMap::new();
    for u in &units {
        for ins in read_instructions(&u.compiled) {
            if ins.opcode != 0x10 || ins.args.len() < 4 {
                continue;
            }
            // The Begin frame: paramCount at 0..2, the event code at 2..4 —
            // no: the parameter count is the FIRST two bytes and the event the
            // second two, so the event is args[0..2] and the count args[2..4].
            // Confirmed against CG02SweetrollSCRIPT, whose three Begins spell
            // OnGrab, OnRelease and OnAdd at args[0..2] as 0x21, 0x22 and 0x03.
            let event = u16::from_le_bytes([ins.args[0], ins.args[1]]);
            shapes.entry(event).or_default().push(ins.args.len());
        }
    }
    for (event, lens) in &shapes {
        let name = nvre_script::opcodes::Event::from_code(*event)
            .map(|e| e.name())
            .unwrap_or("?");
        let distinct: std::collections::BTreeSet<&usize> = lens.iter().collect();
        println!(
            "  0x{event:04X} {name:<26} {} units, arg lengths {:?}",
            lens.len(),
            distinct
        );
    }
}
