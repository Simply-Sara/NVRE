//! Probe: ShowMessage's exact argument bytes across the corpus.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_showmsg
//! ```
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
use nvre_script::emit::read_instructions;
fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        return;
    };
    let path = std::path::Path::new(&data).join("FalloutNV.esm");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), "f".into()) else {
        return;
    };
    let mut shapes: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
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
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        let source = subs
            .iter()
            .find(|x| x.sig == *b"SCTX")
            .map(|x| String::from_utf8_lossy(&x.data).into_owned())
            .unwrap_or_default();
        if compiled.is_empty() {
            return;
        }
        let ins = read_instructions(&compiled);
        let mut k = 0;
        while k < ins.len() {
            let dotted = ins[k].opcode == 0x001C;
            let call = if dotted {
                if k + 1 >= ins.len() {
                    break;
                }
                &ins[k + 1]
            } else {
                &ins[k]
            };
            if call.opcode == 0x1059 {
                let key = format!("{:02X?}", call.args);
                *shapes.entry(key).or_insert(0) += 1;
                if shapes.len() < 6 || shapes.values().sum::<usize>() % 40 == 0 {
                    let line = source
                        .lines()
                        .find(|l| l.to_ascii_lowercase().contains("showmessage"));
                    println!("{edid}: args {:02X?}", call.args);
                    if let Some(l) = line {
                        println!("    src: {l}");
                    }
                }
            }
            k += if dotted { 2 } else { 1 };
        }
    });
    println!("--- distinct arg shapes ---");
    for (k, v) in &shapes {
        println!("  {v:>4}x  {k}");
    }
}
