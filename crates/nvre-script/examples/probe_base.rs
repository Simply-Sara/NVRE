//! Probe: for a placed reference named in a script's SCRO, does its record
//! (or its base form's) carry the script, and through which subrecord?
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_base -- <SCRO-name>
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
    let wanted = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "VFSOrrisThug01REF".into());
    // Find the record whose EDID is the wanted name, print its signature
    // and subrecords; if it has a base-form field (the first formid-valued
    // subrecord that is not EDID/SCRV/SCRL), follow it.
    visit_nodes(&plugin.nodes, &mut |rec, _| {
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
        if !edid.eq_ignore_ascii_case(&wanted) {
            return;
        }
        let sig = String::from_utf8_lossy(&rec.header.sig);
        println!("record {wanted} ({:08X}) sig {sig}", rec.header.form_id);
        for s in subs.iter() {
            let sigsig = String::from_utf8_lossy(&s.sig);
            let data: String = s.data.iter().take(8).map(|b| format!("{b:02X}")).collect();
            println!("  {sigsig} [{data}]");
        }
    });
}
