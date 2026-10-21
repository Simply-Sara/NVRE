//! NVRE's own schema-coverage probe: the same numbers `fnvtool esm coverage`
//! prints, built against NVRE's patched `fnv-esm` so the `len_string` and
//! `count_prefix` schema constructs are exercised. `fnvtool` lives inside the
//! NVForever submodule and links upstream's `fnv-esm` by path, so it cannot see
//! NVRE's `[patch]`.

use fnv_esm::decode::Coverage;
use fnv_esm::raw::visit_records;
use fnv_esm::{Plugin, Schema};

fn main() {
    let mut args = std::env::args().skip(1);
    // `--overlay <dir>`: layer `<dir>` over `--base` instead of reading one
    // directory on its own. A game that shares most of another game's layouts
    // describes only the records that differ, so its coverage is measured
    // against the merged result rather than against a directory that would have
    // to restate every record type.
    //
    // The chain is `base -> <game dirs>`, and each game directory may carry a
    // symlink to `../base` so it can also be read on its own. `load_game`
    // skips files reached through that symlink, so nothing merges twice.
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
    let plugins: Vec<String> = args.collect();
    let schema = match &overlays {
        _ if !overlays.is_empty() => Schema::load_game(&schema_dir, &overlays),
        _ => Schema::load_dir(&schema_dir),
    };
    let schema = match schema {
        Ok(s) => s,
        Err(e) => {
            eprintln!("schema error: {e}");
            std::process::exit(2);
        }
    };
    let mut any_error = 0u64;
    #[allow(unused)]
    let mut by_sig: std::collections::BTreeMap<String, u64> = Default::default();
    let mut by_rec: std::collections::BTreeMap<String, u64> = Default::default();
    let mut sub_sigs: std::collections::BTreeMap<(String, String), u64> = Default::default();
    let mut by_err: std::collections::BTreeMap<(String, String, String), u64> = Default::default();
    for path in &plugins {
        let p = match Plugin::open(path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{path}: {e}");
                std::process::exit(2);
            }
        };
        let mut cov = Coverage::default();
        let mut recs = Vec::new();
        visit_records(&p.nodes[1..], &mut |r, _| recs.push(r));
        cov.add_records_par(&recs, &schema);
        // Which signatures are still unknown, and on which record types. The
        // per-signature totals say what is left to describe; the record
        // breakdown says how much of it the editor ever shows a user.
        // One context per record, so cross-subrecord switches still resolve
        // (a fresh context per subrecord would lose PRKE/EPFT and report
        // false PERK errors).
        for rec in &recs {
            let rsig = rec.sig_str();
            let mut ctx = fnv_esm::decode::DecodeCtx::new(&schema, rec.header.sig);
            let subs = rec.subrecords().expect("subs");
            // The sequence walk, so a record whose subrecords are an ordered
            // list of blocks is measured against the block each subrecord is
            // in — the same walk the decoder and `Coverage::measure` use.
            fnv_esm::decode::walk_sequence(&rec.header.sig, &subs, &schema, &mut ctx, |s, ctx| {
                let d = fnv_esm::decode::decode_sub(&rec.header.sig, s, ctx);
                if d.status.as_str() == "unknown" {
                    let ssig = s.sig_str();
                    *by_sig.entry(ssig.clone()).or_insert(0) += 1;
                    *by_rec.entry(rsig.clone()).or_insert(0) += 1;
                    *sub_sigs.entry((rsig.clone(), ssig)).or_insert(0) += 1;
                }
                if d.status.as_str() == "error" {
                    let ssig = s.sig_str();
                    let msg = d
                        .error
                        .as_ref()
                        .map(|e| format!("{e:?}"))
                        .unwrap_or_else(|| "error".to_string());
                    *by_err.entry((rsig.clone(), ssig, msg)).or_insert(0) += 1;
                }
            });
        }
        let v = cov.to_json(40);
        // The trailing subrecords are a separate list: they decoded, but left
        // bytes over, which means the layout is incomplete rather than wrong.
        let mut trailing: Vec<((String, String), u64, Option<String>)> = cov
            .by_sub
            .iter()
            .filter(|(_, c)| c.trailing > 0)
            .map(|((rt, st), c)| ((rt.clone(), st.clone()), c.trailing, c.first_error.clone()))
            .collect();
        trailing.sort_by_key(|(_, n, _)| std::cmp::Reverse(*n));
        if !trailing.is_empty() {
            println!("\n--- trailing subrecords (decoded, but bytes left over) ---");
            for ((rt, st), n, e) in trailing.iter().take(20) {
                println!("{st:5} {n:>8}  on {rt}{}", e.as_ref().map(|x| format!(": {x}")).unwrap_or_default());
            }
        }
        let s = &v["summary"];
        let total = s["subrecords"].as_u64().unwrap_or(0);
        let ok = s["ok"].as_u64().unwrap_or(0);
        let pct = if total == 0 {
            0.0
        } else {
            ok as f64 * 100.0 / total as f64
        };
        println!(
            "{:<24} total {total} ok {ok} ({pct:.2}%) unknown {} trailing {} error {}",
            p.name, s["unknown"], s["trailing"], s["error"]
        );
        any_error += s["error"].as_u64().unwrap_or(0) + s["trailing"].as_u64().unwrap_or(0);
    }
    // What is still unknown, worst first, and which record types carry it.
    println!("\n--- unknown subrecord signatures (all masters) ---");
    let mut pairs: Vec<_> = sub_sigs.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for ((r, s), n) in pairs.iter().take(25) {
        println!("{s:<6} {n:>6}  on {r}");
    }
    let top_rec: Vec<_> = {
        let mut v: Vec<_> = by_rec.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.into_iter().take(15).collect()
    };
    println!("\n--- unknown count by record type ---");
    for (k, n) in top_rec {
        println!("{k:<6} {n:>6}");
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
