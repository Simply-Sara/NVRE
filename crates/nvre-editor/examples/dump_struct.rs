//! Print the DATA subrecord of every record of a type, laid out as float32
//! and uint32 columns, so a struct's field boundaries can be read off the
//! real data.
//!
//! Usage: `dump_struct <SIG> <SUB> [limit]`

use nvre_editor::session::Session;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: dump_struct <SIG> <SUB> [limit]");
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
        for s in rec.subrecords().expect("subrecords") {
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
            for (i, chunk) in s.data.as_chunks::<4>().0.iter().enumerate() {
                let u = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                let f = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                println!("  [{:3}] {:4}  u={:<12} f={}", i, i * 4, u, f);
            }
            let rem = s.data.len() % 4;
            if rem != 0 {
                println!("  trailing {rem} byte(s)");
            }
        }
    }
}
