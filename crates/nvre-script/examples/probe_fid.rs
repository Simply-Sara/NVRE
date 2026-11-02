//! Probe: print a unit's source and instruction stream by FormID prefix.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_fid -- <hex-formid>
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
    let wanted = u32::from_str_radix(&std::env::args().nth(1).unwrap_or_default(), 16).unwrap_or(0);
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.form_id != wanted {
            return;
        }
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
        println!("=== {edid} ({wanted:08X}) ===");
        for (n, l) in source.lines().enumerate() {
            println!("src{n:>3}| {l}");
        }
        println!("--- instructions ---");
        for ins in read_instructions(&compiled) {
            println!("@{} 0x{:04X} {:02X?}", ins.at, ins.opcode, ins.args);
        }
    });
}
