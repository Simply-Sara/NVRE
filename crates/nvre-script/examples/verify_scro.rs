//! Verify: the reference index in bytecode is the 1-based SCRO position.
//!
//! fopdoc documents `SCRO` as "A local variable reference, or FormID of a
//! ACTI, DOOR, STAT…", and the bytecode spec says a `72 <u16>` argument is a
//! "1-based SCRO index". This checks that rule across the corpus: for every
//! script, the highest reference index used must not exceed its SCRO count.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example verify_scro
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
    let mut no_scro_but_uses = 0usize;
    let mut examples: Vec<String> = Vec::new();
    let mut shown = 0usize;

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
        if compiled.is_empty() {
            return;
        }
        // Every SCRO subrecord holds one FormID; the position is the index.
        let scro: Vec<u32> = subs
            .iter()
            .filter(|x| x.sig == *b"SCRO" && x.data.len() == 4)
            .map(|x| u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]))
            .collect();

        // The highest reference index the bytecode uses. A `72` tag opens a
        // reference argument; walk every instruction's argument bytes.
        let mut max_ref = 0u16;
        let mut uses_ref = false;
        for ins in read_instructions(&compiled) {
            if ins.opcode < 0x1000 {
                continue;
            }
            let mut j = 2usize;
            while j + 2 < ins.args.len() {
                if ins.args[j] == 0x72 {
                    let v = u16::from_le_bytes([ins.args[j + 1], ins.args[j + 2]]);
                    max_ref = max_ref.max(v);
                    uses_ref = true;
                    j += 3;
                } else {
                    j += 1;
                }
            }
        }
        let _ = uses_ref;

        if max_ref == 0 {
            return;
        }
        // For the ones that exceed, print the instruction so the
        // misparse can be seen rather than inferred.
        if max_ref as usize > scro.len() && !scro.is_empty() && shown < 6 {
            shown += 1;
            // Are there SCRO subrecords that are NOT 4 bytes? Those would be
            // packed lists my reader is dropping.
            let sizes: Vec<usize> = subs
                .iter()
                .filter(|x| x.sig == *b"SCRO")
                .map(|x| x.data.len())
                .collect();
            let odd: Vec<&usize> = sizes.iter().filter(|n| **n != 4).collect();
            println!(
                "{edid}: maxRef {max_ref}, {} SCRO of len 4, odd lens {odd:?}, all {:?}",
                scro.len(),
                sizes
            );
        }
        if scro.is_empty() {
            no_scro_but_uses += 1;
            if examples.len() < 10 {
                examples.push(format!("{edid}: uses ref {max_ref} but has 0 SCRO"));
            }
        } else if max_ref as usize <= scro.len() {
            ok += 1;
        } else {
            exceeds += 1;
            if examples.len() < 10 {
                examples.push(format!(
                    "{edid}: uses ref {max_ref} but SCRO has only {}",
                    scro.len()
                ));
            }
        }
    });

    println!("reference index within SCRO count: {ok}");
    println!("reference index exceeds SCRO count: {exceeds}");
    println!("uses a reference but has no SCRO:  {no_scro_but_uses}");
    println!();
    for e in &examples {
        println!("  {e}");
    }
}
