//! Find a subrecord that decodes but does not re-encode to the same bytes.
//!
//! Run with:
//!   NVRE_DATA=<Data> cargo run -p nvre-editor --example rtgap -- <SIG> <SUBSIG>

use fnv_esm::Schema;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let want = std::env::args().nth(1).unwrap_or_default();
    let want_sub = std::env::args().nth(2).unwrap_or_default();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_game(&root.join("schemas/base"), &[root.join("schemas/fo4")])?;

    let bytes = std::fs::read(format!("{data}/Fallout4.esm"))?;
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "Fallout4.esm".into())?;
    let mut shown = 0;
    let mut bad = 0;
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
        if bad >= 5 {
            return;
        }
        let sig = fnv_util::tag_str(&r.header.sig).to_string();
        if sig != want {
            return;
        }
        for s in r.subrecords().expect("subs") {
            let ssig = fnv_util::tag_str(&s.sig).to_string();
            if ssig != want_sub {
                continue;
            }
            let mut ctx = fnv_esm::decode::DecodeCtx::new(&schema, r.header.sig);
            let d = fnv_esm::decode::decode_sub(&r.header.sig, &s, &mut ctx);
            if let Some(v) = &d.value {
                match nvre_edit::encode::encode_sub_len(&schema, &r.header.sig, &s.sig, v, Some(s.data.len())) {
                    Ok(b) => {
                        if b != s.data {
                            bad += 1;
                            println!("MISMATCH len {} : {}", s.data.len(), hex(&s.data[..s.data.len().min(48)]));
                            println!("         encoded  {}: {}", b.len(), hex(&b[..b.len().min(48)]));
                            println!("         decoded: {}", serde_json::to_string(&d.value).unwrap_or_default());
                        }
                    }
                    Err(e) => {
                        bad += 1;
                        println!("ENCODE ERROR: {e}");
                        println!("decoded: {}", serde_json::to_string(&d.value).unwrap_or_default());
                    }
                }
            }
            shown += 1;
            return;
        }
    });
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}
