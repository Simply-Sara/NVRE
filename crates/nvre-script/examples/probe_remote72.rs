//! Probe: the reference index of a remote script variable, measured.
//!
//! `Script.var` compiles to `72 <refIdx> <tag> <varId>` — in expressions
//! (dotted operand), in `set` destinations, and as ShowMessage format
//! values. Which index space is `refIdx`? This probe finds every such
//! compound in the corpus, names the script part from the unit's SCRO list
//! and declared refs (the only names that can precede a dot), and prints
//! the index beside the name's position in each candidate table.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_remote72
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

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

    // EDID/FormID map, to name SCRO entries that are scripts.
    let mut formid_to_edid: std::collections::HashMap<u32, String> = Default::default();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        let Ok(subs) = rec.subrecords() else { return };
        if let Some(e) = subs.iter().find(|x| x.sig == *b"EDID") {
            formid_to_edid.insert(
                rec.header.form_id,
                String::from_utf8_lossy(&e.data)
                    .trim_end_matches('\0')
                    .to_string(),
            );
        }
    });

    // Every SCPT that carries a source: its SCRO names, declared ref names,
    // and the remote compounds in its bytecode.
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
        if compiled.is_empty() || source.is_empty() {
            return;
        }
        let scro: Vec<String> = subs
            .iter()
            .filter(|x| x.sig == *b"SCRO" && x.data.len() == 4)
            .map(|x| {
                let fid = u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]);
                formid_to_edid
                    .get(&fid)
                    .cloned()
                    .unwrap_or_else(|| format!("__form_{fid:08X}"))
            })
            .collect();
        let decl_refs: Vec<String> = source
            .lines()
            .filter_map(|line| {
                let t = line.trim();
                let mut words = t.split_whitespace();
                let first = words.next()?;
                if !first.eq_ignore_ascii_case("ref") && !first.eq_ignore_ascii_case("reference") {
                    return None;
                }
                let name = words.next()?;
                Some(name.trim_end_matches(';').to_string())
            })
            .collect();

        // A remote compound: 72 idx 72? no — 72 idx <tag: 73|66> <id:u16>.
        // Scan every offset; the shape 72 a b 73 c d or 72 a b 66 c d.
        let mut found = 0usize;
        for i in 0..compiled.len().saturating_sub(5) {
            let tag2 = compiled[i + 3];
            if compiled[i] == 0x72 && (tag2 == 0x73 || tag2 == 0x66) {
                let idx = u16::from_le_bytes([compiled[i + 1], compiled[i + 2]]);
                let id = u16::from_le_bytes([compiled[i + 4], compiled[i + 5]]);
                // Name the ref: declared refs first (1-based), then SCRO.
                let name = if (idx as usize) <= decl_refs.len() {
                    format!("decl:{}", decl_refs[idx as usize - 1])
                } else if (idx as usize) <= decl_refs.len() + scro.len() {
                    format!("scro:{}", scro[idx as usize - decl_refs.len() - 1])
                } else {
                    // Past both: the index the game wrote does not fit the
                    // declared-first space. These are the interesting ones.
                    format!("OVERFLOW?{}", idx)
                };
                println!(
                    "{edid}: 72 {idx} {tag2:02X} {id}   ({name})   [decl {} scro {}]",
                    decl_refs.len(),
                    scro.len()
                );
                found += 1;
                if found > 400 {
                    return;
                }
            }
        }
    });
}
