//! Probe: the exact bytes for a negative literal — in a set value and in an
//! if condition — for one 7E unit and one 2D unit.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_neg2
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
    for wanted in ["VStripSS7QuestScript", "VEFR01NCREmergencyRadioQuestSCRIPT"] {
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
            if !edid.eq_ignore_ascii_case(wanted) {
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
                if l.contains('-')
                    && (l.to_ascii_lowercase().contains("set ")
                        || l.to_ascii_lowercase().contains("if ")
                        || l.to_ascii_lowercase().contains("elseif"))
                {
                    println!("src{n}| {l}");
                }
            }
            // print raw bytes around every 7E and every 2D
            for (i, w) in compiled.windows(4).enumerate() {
                if w[1] == 0x7E || (w[1] == 0x2D && w[0] == 0x20) {
                    let lo = i.saturating_sub(6);
                    let hi = (i + 8).min(compiled.len());
                    println!("  @{i}: {:02X?}", &compiled[lo..hi]);
                }
            }
        });
    }
}
