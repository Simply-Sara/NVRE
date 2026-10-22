//! Dump a record type's records: FormID, Editor ID, and every subrecord's
//! signature, length and raw bytes.
//!
//! This is the ground truth for a record editor's fixtures: the exact
//! subrecord order, the exact subrecord lengths, and the exact byte values,
//! without a lossy round-trip through the schema.
//!
//! Run with:
//!   NVRE_DATA=<Data> cargo run -p nvre-editor --example dump_record -- <SIG> [count]

use nvre_esm::Schema;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let want = std::env::args().nth(1).unwrap_or_default();
    let limit: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    let schema_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv");
    let schema = Schema::load_dir(&schema_dir)?;

    let bytes = std::fs::read(format!("{data}/FalloutNV.esm"))?;
    let plugin = nvre_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "FalloutNV.esm".into())?;

    let mut shown = 0;
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        if shown >= limit {
            return;
        }
        let sig = nvre_util::tag_str(&r.header.sig).to_string();
        if sig != want {
            return;
        }
        let Ok(subs) = r.subrecords() else { return };
        let edid = subs
            .iter()
            .find(|s| s.sig == *b"EDID")
            .map(|s| {
                String::from_utf8_lossy(&s.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        println!(
            "--- {sig} {:08X} flags={:08X} edid={edid}",
            r.header.form_id, r.header.flags
        );
        for s in &subs {
            let hex: String = s.data.iter().map(|b| format!("{b:02X}")).collect();
            let mut ctx = nvre_esm::decode::DecodeCtx::new(&schema, r.header.sig);
            let d = nvre_esm::decode::decode_sub(&r.header.sig, s, &mut ctx);
            let rt = nvre_edit::encode::roundtrips(&schema, &r.header.sig, s);
            println!(
                "    {} len={:4} rt={} hex={hex}",
                s.sig_str(),
                s.data.len(),
                if rt { "yes" } else { "NO " },
            );
            if let Some(v) = &d.value {
                let j = v.to_string();
                if j.len() < 300 {
                    println!("        {j}");
                } else {
                    println!("        {}", &j[..300]);
                }
            }
        }
        shown += 1;
    });
    println!("\n({want}: {shown} records)");
    Ok(())
}
