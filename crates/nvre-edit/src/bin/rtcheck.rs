//! Round-trip check without `cargo test`: every subrecord the decoder reads as
//! `Ok` must re-encode to the same bytes. Reports the signatures that cannot,
//! which must be only the known representation limits.

use nvre_esm::raw::visit_nodes;
use nvre_esm::{Plugin, Schema};

fn main() {
    let mut args = std::env::args().skip(1);
    // `--overlay <dir>` reads `<dir>` over `schema_dir`, the same flag
    // `coverage` takes, so a game's round-trip is checked against the merged
    // schema rather than a directory that restates every record.
    let mut overlays: Vec<String> = Vec::new();
    let schema_dir = loop {
        match args.next().as_deref() {
            Some("--overlay") => match args.next() {
                Some(d) => overlays.push(d.to_string()),
                None => panic!("usage: rtcheck [--overlay <dir>]... <schema-dir> <plugin>..."),
            },
            Some(dir) => break dir.to_string(),
            None => panic!("usage: rtcheck [--overlay <dir>]... <schema-dir> <plugin>..."),
        }
    };
    let plugins: Vec<String> = args.collect();
    let schema = if overlays.is_empty() {
        Schema::load_dir(&schema_dir)
    } else {
        Schema::load_game(&schema_dir, &overlays)
    }
    .expect("schema");
    let mut checked = 0usize;
    let mut gaps: std::collections::BTreeMap<String, usize> = Default::default();
    for path in &plugins {
        let p = Plugin::open(path).expect("plugin");
        let localized = p.header.is_localized();
        visit_nodes(&p.nodes, &mut |r, _| {
            let subs = r.subrecords().expect("subs");
            let mut ctx = nvre_esm::decode::DecodeCtx::new(&schema, r.header.sig);
            ctx.localized = localized;
            nvre_edit::encode::clear_exposed();
            nvre_esm::decode::walk_sequence(&r.header.sig, &subs, &schema, &mut ctx, |s, ctx| {
                let d = nvre_esm::decode::decode_sub(&r.header.sig, s, ctx);
                if let (Some(v), Some(def)) = (
                    &d.value,
                    schema.lookup_in_block(&r.header.sig, &s.sig, ctx.block.as_deref()),
                ) {
                    nvre_edit::encode::expose_subrecord(&schema, def, &s.sig, v);
                }
                let Some(value) = d.value else {
                    return;
                };
                checked += 1;
                let written = nvre_edit::encode::encode_sub_len_in_block_variant(
                    &schema,
                    &r.header.sig,
                    &s.sig,
                    &value,
                    Some(s.data.len()),
                    ctx.block.as_deref(),
                    ctx.block_variant_name.as_deref(),
                );
                if !matches!(written, Ok(ref b) if *b == s.data) {
                    *gaps
                        .entry(format!("{}.{}", r.sig_str(), s.sig_str()))
                        .or_insert(0) += 1;
                }
            });
        });
        println!("{path}: {checked} decoded so far");
    }
    println!("total decoded checked: {checked}");
    if gaps.is_empty() {
        println!("ROUNDTRIP: all decoded subrecords re-encode byte-identically");
    } else {
        println!("ROUNDTRIP GAPS:");
        for (k, n) in &gaps {
            println!("  {k} x{n}");
        }
    }
}
