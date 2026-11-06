//! Probe: write every `SCPT` source to one file, so the corpus can be read
//! with grep.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example dump_src -- /tmp/sources.txt
//! ```
//!
//! Each unit is preceded by a `#### <formid> <edid>` line. Nothing in the game
//! better answers "how often does a script spell this, and how" than the
//! sources the GECK itself shipped.
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA");
        return;
    };
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/sources.txt".to_string());
    let plugin_name = std::env::var("NVRE_PLUGIN").unwrap_or_else(|_| "FalloutNV.esm".to_string());
    let Ok(bytes) = std::fs::read(std::path::Path::new(&data).join(&plugin_name)) else {
        eprintln!("cannot read the master");
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), plugin_name) else {
        eprintln!("cannot parse");
        return;
    };
    let mut out = String::new();
    let mut units = 0usize;
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
        let Some(source) = subs.iter().find(|x| x.sig == *b"SCTX") else {
            return;
        };
        units += 1;
        out.push_str(&format!("#### {:08X} {edid}\n", rec.header.form_id));
        out.push_str(&String::from_utf8_lossy(&source.data));
        out.push_str("\n");
    });
    if let Err(e) = std::fs::write(&out_path, &out) {
        eprintln!("cannot write {out_path}: {e}");
        return;
    }
    println!(
        "{units} sources, {} bytes, written to {out_path}",
        out.len()
    );
}
