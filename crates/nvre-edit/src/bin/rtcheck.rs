//! Round-trip check without `cargo test`: every subrecord the decoder reads as
//! `Ok` must re-encode to the same bytes. Reports the signatures that cannot,
//! which must be only the known representation limits.

use fnv_esm::raw::visit_records;
use fnv_esm::{Plugin, Schema};
use nvre_edit::encode::roundtrips;

fn main() {
    let mut args = std::env::args().skip(1);
    let schema_dir = args.next().unwrap();
    let plugins: Vec<String> = args.collect();
    let schema = Schema::load_dir(&schema_dir).expect("schema");
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
