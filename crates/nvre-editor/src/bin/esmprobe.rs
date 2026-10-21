//! Does NVRE's own record reader read a game's masters, and how much of the
//! data does the schema explain?
//!
//! The measure: how many subrecords the schema accounts for, how many it
//! does not know, how many had bytes left over, and how many could not be
//! read.
use nvre_esm::Schema;
use std::path::PathBuf;

fn main() {
    let schema_dir = std::env::args().nth(1).expect("a schema directory");
    let plugins: Vec<String> = std::env::args().skip(2).collect();

    // The schema directory may be a base plus layers: the first is the base,
    // the rest are layers over it.
    let mut dirs = schema_dir.split(':');
    let base = dirs.next().unwrap();
    let layers: Vec<String> = dirs.map(str::to_string).collect();
    let schema = if layers.is_empty() {
        Schema::load_dir(&PathBuf::from(base)).expect("the schema")
    } else {
        Schema::load_game(&PathBuf::from(base), &layers).expect("the schema")
    };

    let mut total = (0u64, 0u64, 0u64, 0u64, 0u64);
    for p in &plugins {
        let path = PathBuf::from(p);
        let data = std::fs::read(&path).expect("the plugin");
        let plugin = nvre_esm::Plugin::from_bytes(&path, &data).expect("the plugin");
        let mut cov = nvre_esm::decode::Coverage::default();
        for r in plugin.records() {
            let (_, rc) = nvre_esm::decode::decode_record(r, &schema);
            cov.add(rc);
        }
        let t = cov.totals();
        println!(
            "{}: total {} ok {} unknown {} trailing {} error {}",
            path.file_name().unwrap().to_string_lossy(),
            t.0,
            t.1,
            t.2,
            t.3,
            t.4
        );
        total = (
            total.0 + t.0,
            total.1 + t.1,
            total.2 + t.2,
            total.3 + t.3,
            total.4 + t.4,
        );
    }
    // Which subrecords are unexplained, by type.
    let mut by_type: std::collections::BTreeMap<String, (u64, u64, u64, u64)> =
        std::collections::BTreeMap::new();
    let mut seen_unknown: std::collections::HashSet<String> = std::collections::HashSet::new();
    for p in &plugins {
        let path = PathBuf::from(p);
        let data = std::fs::read(&path).expect("the plugin");
        let plugin = nvre_esm::Plugin::from_bytes(&path, &data).expect("the plugin");
        for r in plugin.records() {
            let (_, rc) = nvre_esm::decode::decode_record(r, &schema);
            for sc in &rc.subrecords {
                if std::env::var("NVRE_ESM_TRACE").is_ok() {
                    match &sc.status {
                        nvre_esm::decode::SubStatus::Error(e) => {
                            println!("  {}: {}", String::from_utf8_lossy(&sc.sig), e)
                        }
                        nvre_esm::decode::SubStatus::Unknown => {
                            if !seen_unknown.insert(String::from_utf8_lossy(&sc.sig).into_owned()) {
                                continue;
                            }
                            println!(
                                "  {} is unknown, in a record of type {:?}",
                                String::from_utf8_lossy(&sc.sig),
                                String::from_utf8_lossy(&rc.sig)
                            );
                        }
                        _ => {}
                    }
                }
                let e = by_type
                    .entry(String::from_utf8_lossy(&sc.sig).into_owned())
                    .or_default();
                match sc.status {
                    nvre_esm::decode::SubStatus::Ok => e.0 += 1,
                    nvre_esm::decode::SubStatus::Unknown => e.1 += 1,
                    nvre_esm::decode::SubStatus::Trailing { .. } => e.2 += 1,
                    nvre_esm::decode::SubStatus::Error(_) => e.3 += 1,
                }
            }
        }
    }
    let mut v: Vec<_> = by_type.into_iter().collect();
    v.sort_by_key(|(_, e)| std::cmp::Reverse(e.1 + e.2 + e.3));
    println!("the worst unexplained subrecords:");
    for (k, e) in v.iter().take(15) {
        println!(
            "  {k:8} ok {:6} unknown {:6} trailing {:6} error {:6}",
            e.0, e.1, e.2, e.3
        );
    }
    let t = total;
    println!(
        "ALL: total {} ok {} unknown {} trailing {} error {} ({:.2}% explained)",
        t.0,
        t.1,
        t.2,
        t.3,
        t.4,
        if t.0 == 0 {
            0.0
        } else {
            t.1 as f64 * 100.0 / t.0 as f64
        }
    );
}
