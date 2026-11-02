//! Probe: the simplest `set` statements in the corpus, beside their bytes.
//!
//! The emitter is built one statement at a time, and each needs the exact
//! encoding the game wrote. This finds the smallest units that contain a
//! `set` and prints the instruction stream of each, so the shape can be read
//! off the game's own bytes rather than reasoned from a spec.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example dump_sets
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;

/// Whether `u` declares a variable called `name`.
///
/// A declaration is a type keyword, then the name — and the separator is a
/// tab in the shipped units, so the name is the first whitespace-delimited
/// word after the keyword rather than a fixed offset.
fn units_declares(u: &Unit, name: &str) -> bool {
    const KEYWORDS: [&str; 5] = ["short", "long", "int", "float", "ref"];
    u.source.lines().any(|l| {
        let t = l.trim();
        let mut words = t.split_whitespace();
        let Some(first) = words.next() else {
            return false;
        };
        if !KEYWORDS.iter().any(|k| first.eq_ignore_ascii_case(k)) {
            return false;
        }
        words.next().is_some_and(|n| n.eq_ignore_ascii_case(name))
    })
}

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

    // The units whose source has the fewest `set` lines, so the simplest ones
    // come first — one assignment and nothing else to confuse the reading.
    let mut with_sets: Vec<&Unit> = units
        .iter()
        .filter(|u| {
            u.source
                .lines()
                .filter(|l| {
                    l.split_whitespace()
                        .next()
                        .is_some_and(|w| w.eq_ignore_ascii_case("set"))
                })
                .count()
                == 1
        })
        .collect();
    with_sets.sort_by_key(|u| u.compiled.len());

    // How the value's bytes relate to the number the source spells. A single
    // data point cannot tell a tag from a length from a value, so gather many
    // and look at how the tail changes as the number changes.
    println!("=== the value's encoding, as the number changes ===");
    let mut seen: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for u in &units {
        for line in u.source.lines() {
            let t = line.trim();
            let Some(rest) = t.strip_prefix("set").or_else(|| t.strip_prefix("Set")) else {
                continue;
            };
            let Some((_, after)) = rest.split_once(" to ") else {
                continue;
            };
            let value = after.trim().trim_end_matches(';').trim();
            // Only plain integer literals, so nothing else varies.
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
                continue;
            }
            // The instruction for this line, found by the source's own
            // position: the units here have one set, so its 0x15 is the one.
            for ins in read_instructions(&u.compiled) {
                if ins.opcode != 0x15 {
                    continue;
                }
                let tail: Vec<String> = ins.args.iter().map(|b| format!("{b:02X}")).collect();
                let key = value.to_string();
                seen.entry(key).or_insert(tail.join(" "));
            }
        }
    }
    // Group by the value, so the same number's encodings can be compared.
    let mut by_tail: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (value, tail) in &seen {
        by_tail.entry(tail.clone()).or_default().push(value.clone());
    }
    for (tail, values) in by_tail.iter().take(30) {
        println!("  {tail:<40} <- values {}", values.join(", "));
    }

    println!();
    // `set X to SomeCommand` — the value is a command call with no arguments,
    // which is a different encoding from a literal. Print a few so the shape
    // can be read off the game's own bytes.
    println!("=== `set X to <a bare command>` ===");
    let mut shown = 0;
    for u in &units {
        if shown >= 6 {
            break;
        }
        for line in u.source.lines() {
            let t = line.trim();
            let Some(rest) = t.strip_prefix("set").or_else(|| t.strip_prefix("Set")) else {
                continue;
            };
            let Some((_, after)) = rest.split_once(" to ") else {
                continue;
            };
            let value = after.trim().trim_end_matches(';').trim();
            // A single bare word, not a number and not a dotted reference.
            if value.is_empty()
                || value.contains(' ')
                || value.contains('.')
                || value
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_digit() || b == b'-')
            {
                continue;
            }
            // Skip the ones that are just a declared variable of the script:
            // those are the `f`/`s` tags already understood, and the
            // interesting case is a word that is not one of them.
            if units_declares(u, value) {
                continue;
            }
            // The line that declares the RHS's name, if there is one — so the
            // syntax a declaration actually uses is visible rather than guessed.
            for l in u.source.lines() {
                if l.split_whitespace().any(|w| w.eq_ignore_ascii_case(value)) && !l.contains("to")
                {
                    let t = l.trim();
                    if !t.to_ascii_lowercase().starts_with("set") {
                        println!("      (declared as: {t})");
                        break;
                    }
                }
            }
            println!("  {} : set ... to {value}", u.edid);
            println!("      --- first 12 source lines ---");
            for l in u.source.lines().filter(|l| !l.trim().is_empty()).take(12) {
                println!("      | {l}");
            }
            println!("      --- end ---");
            for ins in read_instructions(&u.compiled) {
                if ins.opcode == 0x15 {
                    println!(
                        "      @{:<4} 0x{:04X} args[{}] = {:02X?}",
                        ins.at,
                        ins.opcode,
                        ins.args.len(),
                        ins.args
                    );
                }
            }
            shown += 1;
            break;
        }
    }

    println!();
    println!("=== the 8 smallest units with exactly one `set` ===");
    for u in with_sets.iter().take(8) {
        println!();
        println!("--- {} ---", u.edid);
        println!("source:");
        for line in u.source.lines() {
            if line.trim().is_empty() {
                continue;
            }
            println!("  | {line}");
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
