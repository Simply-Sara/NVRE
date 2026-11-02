//! Probe: follow QUST -> SCRI -> SCPT and print the target script's variable
//! table, verifying the remote IDs the bytecode used.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_scri
//! ```
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;
fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        return;
    };
    let path = std::path::Path::new(&data).join("FalloutNV.esm");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), "f".into()) else {
        return;
    };
    // FormID -> (sig, edid, scri_fid)
    let mut recs: std::collections::HashMap<u32, (String, String, Option<u32>)> =
        Default::default();
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        let Ok(subs) = rec.subrecords() else { return };
        let sig = String::from_utf8_lossy(&rec.header.sig).into_owned();
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        let scri = subs
            .iter()
            .find(|x| x.sig == *b"SCRI")
            .filter(|x| x.data.len() == 4)
            .map(|x| u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]));
        recs.insert(rec.header.form_id, (sig, edid, scri));
    });
    for (fid, (sig, edid, scri)) in &recs {
        if sig != "QUST" {
            continue;
        }
        if !["VFactionSquad", "VNPCFollowers", "BH01", "VMS18"].contains(&edid.as_str()) {
            continue;
        }
        println!("QUST {edid} ({fid:08X}) SCRI -> {scri:?}");
        if let Some(sfid) = scri {
            if let Some((ssig, sedid, _)) = recs.get(sfid) {
                println!("    -> {sfid:08X} sig {ssig} edid {sedid}");
            } else {
                println!("    -> {sfid:08X} [no record]");
            }
        }
    }
    // Also: VFactionSquad's script table (find SCPT with EDID VFactionSquad... or the SCRI target's name)
    println!();
    println!("SCPTs named *FactionSquad* or *Followers*:");
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| {
                String::from_utf8_lossy(&x.data)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        if edid.to_ascii_lowercase().contains("factionsquad")
            || edid.to_ascii_lowercase().contains("followers")
        {
            let names: Vec<String> = subs
                .iter()
                .filter(|x| x.sig == *b"SCVR")
                .map(|x| {
                    String::from_utf8_lossy(&x.data)
                        .trim_end_matches('\0')
                        .to_string()
                })
                .collect();
            println!("  SCPT {edid} ({:08X}) vars: {names:?}", rec.header.form_id);
        }
    });
}
