//! Verify: the reference index counts declared `ref` variables, then SCRO forms.
//!
//! `vCCCrucifiedSlaveSCRIPT` suggests the index space is the script's own
//! declared `ref` variables followed by the record's `SCRO` FormIDs, all
//! 1-based. This checks the rule across the corpus: the highest reference
//! index used must not exceed `declared refs + SCRO entries`.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example verify_refs
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;

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

    let mut ok = 0usize;
    let mut exceeds = 0usize;
    let mut examples: Vec<String> = Vec::new();

    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
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

        // The script's declared `ref` variables, in source order.
        let declared_refs = source
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.split_whitespace()
                    .next()
                    .is_some_and(|w| w.eq_ignore_ascii_case("ref"))
            })
            .count();

        // The record's SCRO FormIDs.
        let scro = subs
            .iter()
            .filter(|x| x.sig == *b"SCRO" && x.data.len() == 4)
            .count();

        // The highest reference index the bytecode uses, from `r` (0x72) and
        // `Z` (0x5A) tags inside instruction arguments.
        let mut max_ref = 0u16;
        for ins in read_instructions(&compiled) {
            if ins.opcode < 0x1000 {
                continue;
            }
            let mut j = 2usize;
            while j + 2 < ins.args.len() {
                if ins.args[j] == 0x72 || ins.args[j] == 0x5A {
                    let v = u16::from_le_bytes([ins.args[j + 1], ins.args[j + 2]]);
                    // A reference index beyond a few thousand is the decoder
                    // reading a tag byte from inside a nested instruction
                    // stream rather than a real index: the corpus's largest
                    // real table is a few hundred entries. Capping it keeps a
                    // misparse from swamping the count.
                    if v < 4096 {
                        max_ref = max_ref.max(v);
                    }
                    j += 3;
                } else {
                    j += 1;
                }
            }
        }
        if max_ref == 0 {
            return;
        }

        let table = declared_refs + scro;
        if max_ref as usize <= table {
            ok += 1;
        } else {
            exceeds += 1;
            if examples.len() < 12 {
                examples.push(format!(
                    "{edid}: maxRef {max_ref} but declaredRefs {declared_refs} + SCRO {scro} = {table}"
                ));
            }
        }
    });

    println!("reference index within declaredRefs + SCRO: {ok}");
    println!("reference index exceeds that:               {exceeds}");
    println!();
    for e in &examples {
        println!("  {e}");
    }
}
