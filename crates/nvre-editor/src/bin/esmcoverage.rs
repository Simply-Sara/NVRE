//! How well does NVRE's own ESM schema explain a game's masters?
//!
//! The same numbers `fnvtool esm coverage` prints, measured with
//! `nvre-esm` rather than upstream's `fnv-esm`.
use nvre_esm::decode::decode_record;
use nvre_esm::raw::visit_records;
use nvre_esm::{Plugin, Schema};
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    // `--overlay <dir>`: layer `<dir>` over the base.
    let mut overlays: Vec<String> = Vec::new();
    let schema_dir = loop {
        match args.next().as_deref() {
            Some("--overlay") => match args.next() {
                Some(d) => overlays.push(d.to_string()),
                None => panic!("--overlay needs a directory"),
            },
            Some(dir) => break dir.to_string(),
            None => break "../../schemas/fnv".to_string(),
        }
    };
    // The first overlay is the base: the schema chain is base, then family,
    // then version.
    let plugins: Vec<String> = args.collect();
    let schema = if overlays.is_empty() {
        Schema::load_dir(&std::path::PathBuf::from(&schema_dir))
    } else {
        // The base comes first, then the layers in the order they were
        // given.
        let mut dirs = overlays.clone();
        let base = dirs.remove(0);
        let mut all = vec![base];
        all.extend(dirs);
        Schema::load_game(
            &std::path::PathBuf::from(&all[0]),
            &all[1..].to_vec(),
        )
    };
    let schema = match schema {
        Ok(s) => s,
        Err(e) => {
            eprintln!("schema error: {e}");
            std::process::exit(2);
        }
    };
    let mut any_error = 0u64;
    let mut by_sig: BTreeMap<String, u64> = Default::default();
    let mut by_rec: BTreeMap<String, u64> = Default::default();
    let mut sub_sigs: BTreeMap<(String, String), u64> = Default::default();
    let mut by_err: BTreeMap<(String, String, String), u64> = Default::default();
    let mut by_trailing: BTreeMap<(String, String), u64> = Default::default();
    for path in &plugins {
        let p = match Plugin::open(std::path::Path::new(path)) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{path}: {e}");
                std::process::exit(2);
            }
        };
        let mut recs = Vec::new();
        let mut nsubs = 0u64;
        visit_records(&p.groups, &mut |r, _| { nsubs += r.subrecords.len() as u64; recs.push(r); });
        eprintln!("the records: {}, the subrecords: {}", recs.len(), nsubs);
        let mut total = 0u64;
        let mut ok = 0u64;
        let mut unknown = 0u64;
        let mut trailing = 0u64;
        let mut error = 0u64;
        for rec in &recs {
            let rsig = rec.sig_str();
            let (_value, cov) = decode_record(rec, &schema);
            total += cov.subrecords.len() as u64;
            ok += cov.ok_count;
            unknown += cov.unknown_count;
            trailing += cov.trailing_count;
            error += cov.error_count;
            for s in &cov.subrecords {
                let ssig = String::from_utf8_lossy(&s.sig).into_owned();
                match &s.status {
                    nvre_esm::decode::SubStatus::Unknown => {
                        *by_sig.entry(ssig.clone()).or_insert(0) += 1;
                        *by_rec.entry(rsig.clone()).or_insert(0) += 1;
                        *sub_sigs.entry((rsig.clone(), ssig)).or_insert(0) += 1;
                    }
                    nvre_esm::decode::SubStatus::Trailing { .. } => {
                        *by_trailing.entry((rsig.clone(), ssig)).or_insert(0) += 1;
                    }
                    nvre_esm::decode::SubStatus::Error(message) => {
                        *by_err
                            .entry((rsig.clone(), ssig, message.clone()))
                            .or_insert(0) += 1;
                    }
                    _ => {}
                }
            }
        }
        let pct = if total == 0 {
            0.0
        } else {
            ok as f64 * 100.0 / total as f64
        };
        println!(
            "{:<24} total {total} ok {ok} ({pct:.2}%) unknown {unknown} trailing {trailing} error {error}",
            p.path.display()
        );
        any_error += error + trailing;
    }
    println!("\n--- unknown subrecord signatures (all masters) ---");
    let mut pairs: Vec<_> = sub_sigs.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for ((r, s), n) in pairs.iter().take(25) {
        println!("{s:<6} {n:>6}  on {r}");
    }
    println!("\n--- unknown count by record type ---");
    let mut top: Vec<_> = by_rec.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (k, n) in top.into_iter().take(15) {
        println!("{k:<6} {n:>6}");
    }
    // the trailing ones
    {
        let mut tr: Vec<((String, String), u64)> = by_trailing.into_iter().collect();
        tr.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        if !tr.is_empty() {
            println!("\n--- trailing subrecords (read, but bytes left over) ---");
            for ((r, sg), n) in tr.iter().take(15) {
                println!("{sg:<6} {n:>6}  on {r}");
            }
        }
    }
    if !by_err.is_empty() {
        println!("\n--- decode errors (all masters) ---");
        let mut errs: Vec<_> = by_err.into_iter().collect();
        errs.sort_by_key(|a| std::cmp::Reverse(a.1));
        for ((r, s, m), n) in errs.iter().take(25) {
            println!("{s:<6} {n:>6}  on {r}: {m}");
        }
    }
    if any_error != 0 {
        std::process::exit(1);
    }
}
