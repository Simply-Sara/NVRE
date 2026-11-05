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
        let Some(rec) = session.resolved_record(*fid) else { continue };
        let Ok(subs) = rec.subrecords() else { continue };
        let Some(edid) = subs.iter().find(|s| &s.sig == b"EDID") else { continue };
        let name = String::from_utf8_lossy(&edid.data).trim_end_matches('\0').to_string();
        if name.starts_with("fAVD") {
            let Some(data_sub) = subs.iter().find(|s| &s.sig == b"DATA") else { continue };
            if data_sub.data.len() == 4 {
                let v = f32::from_le_bytes([data_sub.data[0], data_sub.data[1], data_sub.data[2], data_sub.data[3]]);
                println!("  {name:<35}: {v}");
            }
        }
    }
}
