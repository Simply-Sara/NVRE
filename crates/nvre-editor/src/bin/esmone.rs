//! Decode one record and show what each reader made of it.
use nvre_esm::raw::visit_records;
use nvre_esm::{Plugin, Schema};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut overlays: Vec<String> = Vec::new();
    let schema_dir = loop {
        match args.next().as_deref() {
            Some("--overlay") => overlays.push(args.next().unwrap()),
            Some(d) => break d.to_string(),
            None => break "../../schemas/fnv".to_string(),
        }
    };
    let path = args.next().unwrap();
    let want = args.next().unwrap_or_else(|| "NAVM".to_string());
    let want_sub = std::env::var("WANT_SUB").unwrap_or_else(|_| "NVGD".to_string());
    let schema = if overlays.is_empty() {
        Schema::load_dir(&std::path::PathBuf::from(&schema_dir)).unwrap()
    } else {
        let mut dirs = overlays.clone();
        let base = dirs.remove(0);
        let mut all = vec![base];
        all.extend(dirs);
        Schema::load_game(&std::path::PathBuf::from(&all[0]), &all[1..].to_vec()).unwrap()
    };
    eprintln!("the common subrecords: {}", schema.common_subrecords.len());
    eprintln!("MOD2 in the common: {}", schema.common_subrecords.contains_key("MOD2"));
    eprintln!("MODT in the common: {:?}", schema.common_subrecords.get("MODT").map(|d| format!("{:?}", d.ty)));
    eprintln!("SKYRIM_MODT: {:?}", schema.type_def("SKYRIM_MODT").map(|t| (t.peek_union.is_some(), t.union.len())));
    eprintln!("the records: {}", schema.records.len());
    if let Some(r) = schema.records.get("PACK") {
        eprintln!("PACK's sequence: {}", r.sequence.is_some());
        if let Some(seq) = &r.sequence {
            for (n, b) in &seq.blocks {
                eprintln!("  the block {n}: opener {}, {} subs, {} variants", b.opener, b.subrecords.len(), b.variants.len());
            }
        }
    }
    eprintln!("ARMO's subrecords: {:?}", schema.records.get("ARMO").map(|r| r.subrecords.keys().collect::<Vec<_>>()));
    let p = Plugin::open(std::path::Path::new(&path)).unwrap();
    let mut n = 0;
    visit_records(&p.groups, &mut |r, _| {
        if r.sig_str() != want {
            return;
        }
        // only the record that holds the subrecord we are after
        let subs0 = r.subrecords().unwrap();
        if !subs0.iter().any(|s| s.sig_str() == want_sub) {
            return;
        }
        n += 1;
        if n > 1 {
            return;
        }
        let subs = r.subrecords().unwrap();
        for s in &subs {
            println!("{:?} {} bytes", s.sig_str(), s.data.len());
        }
        for s in &subs {
            if s.sig_str() == want_sub {
                println!("the bytes: {} {:?}", s.data.len(), &s.data[..s.data.len().min(20)]);
            }
        }
        // the failing one
        for s in &subs {
            if s.sig_str() == want_sub {
                let (v, cov) = nvre_esm::decode::decode_record(r, &schema);
                println!("the record: {}", serde_json::to_string(&v).unwrap_or_default().len());
                for sc in &cov.subrecords {
                    println!("  {:?} {:?}", String::from_utf8_lossy(&sc.sig), sc.status);
                }
                return;
            }
        }
    });
}
