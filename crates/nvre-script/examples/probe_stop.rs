//! Probe: how does the game encode StopCombat/RemoveScriptPackage calls,
//! dotted and bare, with and without an explicit argument?
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_stop
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
    let mut shown = 0;
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" || shown >= 12 {
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
        if compiled.is_empty() {
            return;
        }
        // find 17 10 and 98 10 sequences
        for w in compiled.windows(6) {
            if (w[0] == 0x17 && w[1] == 0x10) || (w[0] == 0x98 && w[1] == 0x10) {
                // context: 8 bytes before (may include 1C prefix), 8 after
                let at = compiled.windows(6).position(|x| x == w).unwrap_or(0);
                let _ = at;
            }
        }
        // simpler: scan byte positions
        let mut i = 0;
        while i + 6 <= compiled.len() {
            let op = u16::from_le_bytes([compiled[i], compiled[i + 1]]);
            if op == 0x1017 || op == 0x1098 {
                let before = if i >= 6 {
                    &compiled[i - 6..i]
                } else {
                    &compiled[0..i]
                };
                let after = &compiled[i..(i + 10).min(compiled.len())];
                println!("{edid}: ...{:02X?} | {:02X?}...", before, after);
                shown += 1;
                if shown >= 12 {
                    return;
                }
            }
            i += 1;
        }
    });
}
