//! Probe: the SCRI target of a placed ref's base — its EDID and variable table.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_scri2 -- <formid-hex>
//! ```
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
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
    let wanted = u32::from_str_radix(
        &std::env::args().nth(1).unwrap_or_else(|| "118A77".into()),
        16,
    )
    .unwrap();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.form_id != wanted {
            return;
        }
        let sig = String::from_utf8_lossy(&rec.header.sig);
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
        println!("{:08X} {sig} edid {edid}", rec.header.form_id);
        if sig == "SCPT" {
            let names: Vec<String> = subs
                .iter()
                .filter(|x| x.sig == *b"SCVR")
                .map(|x| {
                    String::from_utf8_lossy(&x.data)
                        .trim_end_matches('\0')
                        .to_string()
                })
                .collect();
            println!("  vars: {names:?}");
        }
    });
}
