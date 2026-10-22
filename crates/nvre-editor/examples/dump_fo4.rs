//! Dump one Fallout 4 record's subrecords and the bytes of one of them.
//!
//! Run with:
//!   NVRE_DATA=<Data> cargo run -p nvre-editor --example dump_fo4 -- <SIG> <SUBSIG> [count]

use fnv_esm::Schema;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let want = std::env::args().nth(1).unwrap_or_default();
    let want_sub = std::env::args().nth(2).unwrap_or_default();
    let limit: usize = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_game(&root.join("schemas/base"), &[root.join("schemas/fo4")])?;

    let path = format!("{data}/Fallout4.esm");
    let bytes = std::fs::read(&path)?;
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "Fallout4.esm".into())?;

    let mut shown = 0;
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
        if shown >= limit {
            return;
        }
        let sig = fnv_util::tag_str(&r.header.sig).to_string();
        if sig != want {
            return;
        }
        let Ok(subs) = r.subrecords() else { return };
        for s in &subs {
            let ssig = fnv_util::tag_str(&s.sig).to_string();
            if ssig != want_sub {
                continue;
            }
            println!("--- record {} subrecord {} len {}", sig, ssig, s.data.len());
            println!("    {}", hex(&s.data[..s.data.len().min(96)]));
            let d = fnv_esm::decode::decode_record(r, &schema);
            println!("{}", serde_json::to_string(&d).unwrap_or_default());
            shown += 1;
            return;
        }
    });
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}
