use nvre_editor::session::Session;

fn main() {
    let data = std::env::var("NVRE_DATA").expect("NVRE_DATA");
    let session = Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .expect("session");

    for (fid, _) in session.records_of(*b"GMST").iter() {
        let Some(rec) = session.resolved_record(*fid) else {
            continue;
        };
        let Ok(subs) = rec.subrecords() else { continue };
        let Some(edid) = subs.iter().find(|s| &s.sig == b"EDID") else {
            continue;
        };
        let name = String::from_utf8_lossy(&edid.data);
        if name.starts_with('i') || name.starts_with('f') {
            if name.to_lowercase().contains("evil") || name.to_lowercase().contains("good") {
                let data_sub = subs.iter().find(|s| &s.sig == b"DATA");
                println!("GMST: {name} data: {data_sub:?}");
            }
        }
    }
}
