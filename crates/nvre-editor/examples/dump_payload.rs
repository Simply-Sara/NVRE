//! Dump one record's subrecord payloads in full, for a type and a signature.
//!
//! Usage: `dump_payload <SIG> <SUB> [limit] [bytes]`

use nvre_editor::session::Session;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: dump_payload <SIG> <SUB> [limit] [bytes]");
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
    let width: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(64);

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
        for s in &subs {
            if s.sig != want {
                continue;
            }
            if shown >= limit {
                return;
            }
            shown += 1;
            println!(
                "--- {fid:08X} {} len={} ---",
                fnv_util::tag_str(&want),
                s.data.len()
            );
            for (i, b) in s.data.iter().enumerate() {
                if i % width == 0 {
                    print!("  {:4}: ", i);
                }
                print!("{b:02X} ");
                if i % width == width - 1 {
                    println!();
                }
            }
            if s.data.len() % width != 0 {
                println!();
            }
        }
    }
}
