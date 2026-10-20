//! Show exactly what a type's save changes, for the records that differ.
//!
//! Usage: `diff_type <SIG> [limit]`

use nvre_editor::object_editor::ObjectEditorState;
use nvre_editor::session::Session;

fn shape(subs: &[fnv_esm::Subrecord]) -> Vec<([u8; 4], Vec<u8>)> {
    subs.iter().map(|s| (s.sig, s.data.clone())).collect()
}

fn show(tag: &str, sig: &[u8; 4], data: &[u8]) {
    let hex: String = data
        .iter()
        .take(64)
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    let text = String::from_utf8_lossy(data);
    let printable = text.chars().all(|c| c.is_ascii_graphic() || c == '\0') && data.len() <= 64;
    let extra = if printable {
        format!("  |{text}|")
    } else {
        String::new()
    };
    println!(
        "  {tag} {:4} len={:<5} {hex}{extra}",
        fnv_util::tag_str(sig),
        data.len()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: diff_type <SIG> [limit]");
        std::process::exit(2);
    }
    let sig: [u8; 4] = [
        args[1].as_bytes()[0],
        *args[1].as_bytes().get(1).unwrap_or(&b' '),
        *args[1].as_bytes().get(2).unwrap_or(&b' '),
        *args[1].as_bytes().get(3).unwrap_or(&b' '),
    ];
    let limit: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);

    let data = std::env::var("NVRE_DATA").expect("NVRE_DATA");
    let schema = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv");
    let mut session = Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(schema),
        vec!["FalloutNV.esm".to_string()],
    )
    .expect("open the master");

    let mut before = std::collections::BTreeMap::new();
    for (fid, rec) in session.doc.records() {
        if rec.header.sig != sig {
            continue;
        }
        before.insert(fid, shape(&rec.subrecords().expect("subrecords")));
    }

    let fids: Vec<u32> = before.keys().copied().collect();
    for fid in &fids {
        let mut state = ObjectEditorState::default();
        state.load_from_session(&session, *fid, sig);
        state.save_to_session(&mut session);
    }

    let mut shown = 0usize;
    for fid in &fids {
        let b = &before[fid];
        let mut rec = session.doc.record(*fid).expect("record").clone();
        if let Some(edits) = session.doc.edits_for(*fid) {
            nvre_edit::document::apply_edits_pub(&mut rec, edits);
        }
        let a = shape(&rec.subrecords().expect("subrecords"));
        if a == *b {
            continue;
        }
        if shown >= limit {
            break;
        }
        shown += 1;
        println!("--- {fid:08X} ---");
        let n = b.len().max(a.len());
        for i in 0..n {
            let bs = b.get(i);
            let as_ = a.get(i);
            if bs == as_ {
                if let Some(s) = bs {
                    show("same", &s.0, &s.1);
                }
                continue;
            }
            if let Some(s) = bs {
                show("BEFORE", &s.0, &s.1);
            }
            if let Some(s) = as_ {
                show("AFTER ", &s.0, &s.1);
            }
        }
    }
}
