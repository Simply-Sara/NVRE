//! Find a subrecord that decodes but does not re-encode to the same bytes.
//!
//! Run with:
//!   NVRE_DATA=<Data> NVRE_OVERLAY=<dir> NVRE_FILE=<name> \
//!     cargo run -p nvre-editor --example rtgap -- <SIG> <SUBSIG>

use nvre_esm::Schema;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NVRE_DATA")?;
    let want = std::env::args().nth(1).unwrap_or_default();
    let want_sub = std::env::args().nth(2).unwrap_or_default();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let overlays: Vec<String> = std::env::var("NVRE_OVERLAY")
        .unwrap_or_else(|_| "schemas/fo4".into())
        .split(',')
        .map(|d| root.join(d).to_string_lossy().into_owned())
        .collect();
    let schema = Schema::load_game(root.join("schemas/base"), &overlays)?;

    let file = std::env::var("NVRE_FILE").unwrap_or_else(|_| "Fallout4.esm".into());
    let path = format!("{data}/{file}");
    let bytes = std::fs::read(&path)?;
    let plugin = nvre_esm::Plugin::from_bytes(std::path::Path::new(&path), &bytes)?;
    let localized = plugin.header.is_localized();
    let mut bad = 0;
    nvre_esm::raw::visit_nodes(&plugin.nodes[1..], &mut |r, _| {
        if bad >= 3 {
            return;
        }
        let sig = nvre_util::tag_str(&r.header.sig);
        if sig != want {
            return;
        }
        let subs = r.subrecords().expect("subs");
        // The walk says which block each subrecord falls in.
        let mut ctx = nvre_esm::decode::DecodeCtx::new(&schema, r.header.sig);
        ctx.localized = localized;
        nvre_edit::encode::clear_exposed();
        nvre_esm::decode::walk_sequence(&r.header.sig, &subs, &schema, &mut ctx, |s, c| {
            let d = nvre_esm::decode::decode_sub(&r.header.sig, s, c);
            if let (Some(v), Some(def)) = (
                &d.value,
                schema.lookup_in_block(&r.header.sig, &s.sig, c.block.as_deref()),
            ) {
                nvre_edit::encode::expose_subrecord(&schema, def, &s.sig, v);
            }
            if nvre_util::tag_str(&s.sig) != want_sub {
                return;
            }
            let Some(v) = &d.value else {
                return;
            };
            match nvre_edit::encode::encode_sub_len_in_block_variant(
                &schema,
                &r.header.sig,
                &s.sig,
                v,
                Some(s.data.len()),
                c.block.as_deref(),
                c.block_variant_name.as_deref(),
            ) {
                Ok(b) => {
                    if b != s.data {
                        bad += 1;
                        println!(
                            "MISMATCH len {} : {}",
                            s.data.len(),
                            hex(&s.data[..s.data.len().min(48)])
                        );
                        println!(
                            "         encoded  {}: {}",
                            b.len(),
                            hex(&b[..b.len().min(48)])
                        );
                        println!(
                            "         decoded: {}",
                            serde_json::to_string(&d.value).unwrap_or_default()
                        );
                    }
                }
                Err(e) => {
                    bad += 1;
                    println!("ENCODE ERROR: {e}");
                    println!(
                        "decoded: {}",
                        serde_json::to_string(&d.value).unwrap_or_default()
                    );
                }
            }
        });
    });
    Ok(())
}

fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
