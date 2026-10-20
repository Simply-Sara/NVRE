//! Round-trip check without `cargo test`: every subrecord the decoder reads as
//! `Ok` must re-encode to the same bytes. Reports the signatures that cannot,
//! which must be only the known representation limits.

use fnv_esm::raw::visit_records;
use fnv_esm::{Plugin, Schema};
use nvre_edit::encode::roundtrips;

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
        visit_records(&p.nodes[1..], &mut |r, _| {
            for s in r.subrecords().expect("subs") {
                let mut ctx = fnv_esm::decode::DecodeCtx::new(&schema, r.header.sig);
                let d = fnv_esm::decode::decode_sub(&r.header.sig, &s, &mut ctx);
                // Only subrecords the decoder actually read can round-trip; an
                // unknown one has no layout to invert, and an errored one has no
                // value at all.
                if d.value.is_none() {
                    continue;
                }
                checked += 1;
                if !roundtrips(&schema, &r.header.sig, &s) {
                    *gaps
                        .entry(format!("{}.{}", r.sig_str(), s.sig_str()))
                        .or_insert(0) += 1;
                }
            }
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
