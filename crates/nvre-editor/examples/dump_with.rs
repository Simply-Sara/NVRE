//! Dump the raw subrecord sequence of records of a type that carry a given
//! subrecord, so an interleaved collection's ordering can be read off the
//! real data.
//!
//! Usage: `dump_with <SIG> <SUB> [limit]`

use nvre_editor::session::Session;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: dump_with <SIG> <SUB> [limit]");
        std::process::exit(2);
    }
    let sig: [u8; 4] = [
        args[1].as_bytes()[0],
        *args[1].as_bytes().get(1).unwrap_or(&b' '),
        *args[1].as_bytes().get(2).unwrap_or(&b' '),
        *args[1].as_bytes().get(3).unwrap_or(&b' '),
    ];
    let want: [u8; 4] = [
        args[2].as_bytes()[0],
        *args[2].as_bytes().get(1).unwrap_or(&b' '),
        *args[2].as_bytes().get(2).unwrap_or(&b' '),
        *args[2].as_bytes().get(3).unwrap_or(&b' '),
    ];
    let limit: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);

    let data = std::env::var("NVRE_DATA").expect("NVRE_DATA");
    let schema = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv");
    let session = Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(schema),
        vec!["FalloutNV.esm".to_string()],
    )
    .expect("open the master");

    let mut shown = 0usize;
    for (fid, rec) in session.doc.records() {
        if rec.header.sig != sig {
            continue;
        }
        let subs = rec.subrecords().expect("subrecords");
        if !subs.iter().any(|s| s.sig == want) {
            continue;
        }
        if shown >= limit {
            break;
        }
        shown += 1;
        println!("--- {fid:08X} ---");
        for s in &subs {
            let hex: String = s
                .data
                .iter()
                .take(32)
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!(
                "  {:4} len={:<5} {}",
                fnv_util::tag_str(&s.sig),
                s.data.len(),
                hex
            );
        }
    }
}
