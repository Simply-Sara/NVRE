//! Probe: does the ESM walker yield GLOB records, and what does VNight's
//! SCRO FormID resolve to?
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_glob
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
    let mut globs = 0usize;
    let mut vnight: Option<(u32, String)> = None;
    let mut sigs: std::collections::BTreeMap<String, usize> = Default::default();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        let sig = String::from_utf8_lossy(&rec.header.sig).into_owned();
        *sigs.entry(sig.clone()).or_insert(0) += 1;
        if sig == "GLOB" {
            globs += 1;
            let Ok(subs) = rec.subrecords() else { return };
            if let Some(e) = subs.iter().find(|x| x.sig == *b"EDID") {
                let name = String::from_utf8_lossy(&e.data)
                    .trim_end_matches('\0')
                    .to_string();
                if name.eq_ignore_ascii_case("VNight") {
                    vnight = Some((rec.header.form_id, name));
                }
            }
        }
    });
    println!("GLOB records: {globs}");
    println!("VNight: {vnight:?}");
    println!("signatures seen: {}", sigs.len());
    let _ = sigs;
}
