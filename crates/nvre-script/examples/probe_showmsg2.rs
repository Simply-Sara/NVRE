//! Probe: ShowMessage's compiled form, source beside bytes, corpus-wide.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_showmsg2
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
        if rec.header.sig != *b"SCPT" || shown >= 14 {
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
        let lines: Vec<&str> = source.lines().collect();
        for (k, x) in ins.iter().enumerate() {
            let op = x.opcode;
            let is_sm =
                op == 0x1059 || (op == 0x001C && k + 1 < ins.len() && ins[k + 1].opcode == 0x1059);
            if !is_sm {
                continue;
            }
            // find the source line: search for ShowMessage near this instruction
            // by matching the message ref name in SCRO order is heavy; just
            // print all showmessage lines with the args.
            let sm_lines: Vec<String> = lines
                .iter()
                .enumerate()
                .filter(|(_, l)| l.to_ascii_lowercase().contains("showmessage"))
                .map(|(n, l)| format!("src{n}: {l}"))
                .collect();
            println!("{edid} ins@{k} args {:02X?}", x.args);
            for l in &sm_lines {
                println!("    {l}");
            }
            shown += 1;
            break;
        }
    });
}
