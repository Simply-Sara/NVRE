//! Probe: pair a command's written argument with the name the source spelled.
//!
//! ```text
//! NVRE_DATA=<Data> [NVRE_PLUGIN=x.esm] cargo run -p nvre-script --example pair_args -- 1013 PlayGroup
//! ```
//!
//! The nth call in a unit's source is the nth instruction of that opcode in its
//! bytecode (the same positional argument the `Begin` codes were settled with),
//! so a code that appears in the bytes can be tied to the name that produced it
//! — which is the only way to fill in an enum the table does not carry yet.
//! Prints one `name -> code` vote per call and the total per code.
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;
use std::collections::BTreeMap;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA");
        return;
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(op_hex) = args.first() else {
        eprintln!("usage: pair_args <opcode hex> <command name>");
        return;
    };
    let Ok(opcode) = u16::from_str_radix(op_hex.trim_start_matches("0x"), 16) else {
        eprintln!("bad opcode {op_hex}");
        return;
    };
    let Some(command) = args.get(1) else {
        eprintln!("usage: pair_args <opcode hex> <command name> [param index] [param bytes]");
        return;
    };
    // A declared-parameter command frames its arguments as `[count:u16]
    // [args…]`, so the first parameter starts at offset 2. The width is what
    // decides how to read it: a code is 2 bytes, an `n`-tagged integer 5.
    let param: usize = args
        .get(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let width: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(2);
    let plugin_name = std::env::var("NVRE_PLUGIN").unwrap_or_else(|_| "FalloutNV.esm".to_string());
    let Ok(bytes) = std::fs::read(std::path::Path::new(&data).join(&plugin_name)) else {
        eprintln!("cannot read the master");
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), plugin_name) else {
        eprintln!("cannot parse");
        return;
    };

    // name -> code -> count, plus every disagreement so a bad pairing is visible.
    let mut votes: BTreeMap<String, BTreeMap<u16, usize>> = BTreeMap::new();
    let mut unpaired = 0usize;
    let mut paired = 0usize;
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| String::from_utf8_lossy(&x.data).trim_end_matches('\0').to_string())
            .unwrap_or_default();
        let Some(source) = subs.iter().find(|x| x.sig == *b"SCTX") else {
            return;
        };
        let Some(compiled) = subs.iter().find(|x| x.sig == *b"SCDA") else {
            return;
        };
        let source = String::from_utf8_lossy(&source.data);
        // The first argument of each `Command <name>` line, in source order.
        let mut names: Vec<String> = Vec::new();
        for line in source.lines() {
            let line = line.split(';').next().unwrap_or("");
            let lowered = line.to_ascii_lowercase();
            let needle = command.to_ascii_lowercase();
            let Some(at) = lowered.find(&needle) else {
                continue;
            };
            let before = lowered[..at].trim_end();
            // A dotted call (`ref.Command`) or a statement start only.
            let ok_prefix = before.is_empty()
                || before.ends_with('.')
                || line[..at].trim_start().is_empty()
                || before.ends_with('\t');
            let _ = ok_prefix;
            let rest = &line[at + needle.len()..];
            let Some(first) = rest.split_whitespace().next() else {
                continue;
            };
            // `Ref.Command <arg>`, `Command <arg>` — but not `IsAnimPlaying`,
            // whose name merely contains the command's.
            if line[..at].trim_end().ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                continue;
            }
            names.push(first.trim_end_matches([',', ';']).to_string());
        }
        let ins: Vec<&nvre_script::emit::Instruction> = read_instructions(&compiled.data)
            .into_iter()
            .filter(|i| i.opcode == opcode)
            .collect::<Vec<_>>()
            .into_iter()
            .map(|i| Box::leak(Box::new(i)) as &nvre_script::emit::Instruction)
            .collect();
        if names.len() != ins.len() {
            unpaired += names.len().abs_diff(ins.len());
            eprintln!(
                "{edid}: {} names vs {} instructions",
                names.len(),
                ins.len()
            );
            return;
        }
        for (name, one) in names.iter().zip(ins.iter()) {
            // The first argument is the code for `AnimGroup`/`ActorValue`
            // positions and the `n`-tagged word for an `Integer` one.
            let mut at = 2usize;
            for _ in 0..param {
                at += width;
            }
            if one.args.len() < at + 2 {
                continue;
            }
            let code = if one.args.get(at) == Some(&0x6E) {
                if one.args.len() < at + 3 {
                    continue;
                }
                u16::from_le_bytes([one.args[at + 1], one.args[at + 2]])
            } else {
                u16::from_le_bytes([one.args[at], one.args[at + 1]])
            };
            paired += 1;
            *votes
                .entry(name.clone())
                .or_default()
                .entry(code)
                .or_insert(0) += 1;
        }
    });

    for (name, codes) in &votes {
        let rendered: Vec<String> = codes
            .iter()
            .map(|(code, n)| format!("{code} x{n}"))
            .collect();
        println!("{name:<28} {}", rendered.join(", "));
    }
    println!();
    println!("{paired} names paired, {unpaired} unpaired");
}
