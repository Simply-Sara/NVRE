//! Round-trip check without `cargo test`: every subrecord the decoder reads as
//! `Ok` must re-encode to the same bytes. Reports the signatures that cannot,
//! which must be only the known representation limits.

use fnv_esm::raw::visit_records;
use fnv_esm::{Plugin, Schema};

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
        // A localized file holds its translatable strings as 4-byte string IDs
        // rather than as text, so the decoder has to be told which it is.
        let localized = p.header.is_localized();
        visit_records(&p.nodes[1..], &mut |r, _| {
            let subs = r.subrecords().expect("subs");
            // The walk says which block each subrecord falls in, and a
            // signature may mean one thing in the record's own map and
            // another inside a block, so the subrecords are read in that
            // walk rather than one by one.
            let mut ctx = fnv_esm::decode::DecodeCtx::new(&schema, r.header.sig);
            ctx.localized = localized;
            nvre_edit::encode::clear_exposed();
            fnv_esm::decode::walk_sequence(&r.header.sig, &subs, &schema, &mut ctx, |s, ctx| {
                let d = fnv_esm::decode::decode_sub(&r.header.sig, s, ctx);
                // A subrecord a later one switches on is remembered, as the
                // reader remembers it.
                if let (Some(v), Some(def)) = (
                    &d.value,
                    schema.lookup_in_block(&r.header.sig, &s.sig, ctx.block),
                ) {
                    nvre_edit::encode::expose_subrecord(&schema, def, &s.sig, v);
                }
                // Only subrecords the decoder actually read can round-trip; an
                // unknown one has no layout to invert, and an errored one has no
                // value at all.
                let Some(value) = d.value else {
                    return;
                };
                checked += 1;
                // The write says which block the subrecord is in, so the
                // value is written back the way it was read.
                let written = nvre_edit::encode::encode_sub_len_in_block_variant(
                    &schema,
                    &r.header.sig,
                    &s.sig,
                    &value,
                    Some(s.data.len()),
                    ctx.block,
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
