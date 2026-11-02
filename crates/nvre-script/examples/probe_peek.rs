//! Probe: dump a unit's expected instruction stream with source lines.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_peek -- <EDID>
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
    let wanted = std::env::args().nth(1).unwrap_or_default();
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
        if !edid.eq_ignore_ascii_case(&wanted) {
            return;
        }
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
        println!("=== {edid} ===");
        for (n, l) in source.lines().enumerate() {
            println!("src{n:>3}| {l}");
        }
        println!("--- bytes ---");
        for (n, x) in read_instructions(&compiled).iter().enumerate() {
            println!("ins[{n}] @{} 0x{:04X} {:02X?}", x.at, x.opcode, x.args);
        }
    });
}
