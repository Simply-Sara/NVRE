//! Probe: ShowMessage calls WITH format arguments — source beside bytes.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_showmsg3
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
    let mut shown = 0;
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" || shown >= 20 {
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
        if compiled.is_empty() {
            return;
        }
        let ins = read_instructions(&compiled);
        for (k, x) in ins.iter().enumerate() {
            if x.opcode != 0x1059 {
                continue;
            }
            // only the interesting ones: more than 11 bytes of args
            if x.args.len() <= 11 {
                continue;
            }
            println!("{edid} ins@{k} args {:02X?}", x.args);
            // print showmessage source lines
            for (n, l) in source.lines().enumerate() {
                if l.to_ascii_lowercase().contains("showmessage") {
                    println!("    src{n}: {l}");
                }
            }
            shown += 1;
            break;
        }
    });
}
