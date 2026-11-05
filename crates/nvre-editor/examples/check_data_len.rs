use nvre_editor::session::Session;

fn main() {
    let data = std::env::var("NVRE_DATA").expect("NVRE_DATA");
    let session = Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .expect("session");

    let mut lens = std::collections::BTreeSet::new();
    for (fid, _) in session.records_of(*b"CREA").iter() {
        let Some(rec) = session.resolved_record(*fid) else { continue };
        let Ok(subs) = rec.subrecords() else { continue };
        let Some(data_sub) = subs.iter().find(|s| &s.sig == b"DATA") else { continue };
        lens.insert(data_sub.data.len());
    }
    println!("CREA DATA lengths: {:?}", lens);
}
