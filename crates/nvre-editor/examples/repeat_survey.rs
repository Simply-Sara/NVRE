//! Which subrecord signatures a record type repeats.
//!
//! A page shows a repeated subrecord badly unless it uses the `repeat` widget: the
//! grid draws one row per occurrence with no way to add or remove one, which is
//! most of what a leveled list or a package needs. Deciding which sections need the
//! widget by reading the schema is a guess — the layout says a signature exists,
//! not how often a record carries it — so this counts the real records.
//!
//! Prints one line per repeated signature:
//!
//! ```text
//! {"sig":"PACK","sub":"PKTO","max":9,"records":40,"with_more_than_one":31}
//! ```
//!
//! Usage:
//!   repeat_survey --game fnv [--sig PACK ...] [--limit 300]

use nvre_editor::game::GameProfile;

fn main() {
    let mut game = String::from("fnv");
    let mut only: Vec<String> = Vec::new();
    let mut limit = 300usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--game" => game = args.next().unwrap_or(game),
            "--sig" => {
                if let Some(v) = args.next() {
                    only.push(v)
                }
            }
            "--limit" => limit = args.next().and_then(|v| v.parse().ok()).unwrap_or(limit),
            other => {
                eprintln!("unknown {other:?}");
                std::process::exit(2);
            }
        }
    }
    let profile = GameProfile::by_id(&game).expect("a game");
    let data = profile.data_dir().expect("a data folder");
    let session = nvre_editor::session::Session::open(
        &data.join(profile.master),
        &profile.schema_dirs().0,
        vec![profile.master.to_string()],
    )
    .expect("a session");
    let lo = session.load_order().expect("a load order");

    // (record sig, subrecord sig) -> (max occurrences, records seen, records with >1)
    let mut tallies: std::collections::BTreeMap<([u8; 4], [u8; 4]), (usize, usize, usize)> =
        Default::default();
    let mut sampled: std::collections::BTreeMap<[u8; 4], usize> = Default::default();
    for (pi, _) in lo.plugins.iter().enumerate() {
        for rec in lo.records[pi].iter().copied() {
            if !only.is_empty() {
                let key = rec.sig_str();
                if !only.iter().any(|s| s.eq_ignore_ascii_case(&key)) {
                    continue;
                }
            }
            let n = sampled.entry(rec.header.sig).or_insert(0);
            if *n >= limit {
                continue;
            }
            *n += 1;
            let Ok(subs) = rec.subrecords() else { continue };
            let mut counts: std::collections::BTreeMap<[u8; 4], usize> = Default::default();
            for s in &subs {
                *counts.entry(s.sig).or_insert(0) += 1;
            }
            for (sig, count) in counts {
                let e = tallies.entry((rec.header.sig, sig)).or_insert((0, 0, 0));
                e.0 = e.0.max(count);
                e.1 += 1;
                if count > 1 {
                    e.2 += 1;
                }
            }
        }
    }
    for ((rec_sig, sub_sig), (max, records, multi)) in tallies {
        if max < 2 {
            continue;
        }
        println!(
            "{}",
            serde_json::json!({
                "sig": nvre_util::tag_str(&rec_sig),
                "sub": nvre_util::tag_str(&sub_sig),
                "max": max,
                "records": records,
                "with_more_than_one": multi,
            })
        );
    }
}
