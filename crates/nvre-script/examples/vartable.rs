//! Probe: a script's own `SLSD`/`SCVR` table, as the record writes it.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example vartable -- DLC05AlienTechFunctionsScript
//! ```
//!
//! The compiler takes this table from the caller, and a table that is short by
//! one entry makes every variable id after the gap wrong. This prints every
//! `SLSD` (its id, the `IsLongOrShort` flag) beside the `SCVR` name at the same
//! position, plus a count on each side, so a mismatch is visible rather than
//! silently zipped.
use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA");
        return;
    };
    let Some(wanted) = std::env::args().nth(1) else {
        eprintln!("pass a script EDID");
        return;
    };
    let plugin_name = std::env::var("NVRE_PLUGIN").unwrap_or_else(|_| "FalloutNV.esm".to_string());
    let Ok(bytes) = std::fs::read(std::path::Path::new(&data).join(&plugin_name)) else {
        eprintln!("cannot read the master");
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), plugin_name) else {
        eprintln!("cannot parse");
        return;
    };
    visit_nodes(&plugin.nodes, &mut |rec, _| {
        if rec.header.sig != *b"SCPT" {
            return;
        }
        let Ok(subs) = rec.subrecords() else { return };
        let edid = subs
            .iter()
            .find(|x| x.sig == *b"EDID")
            .map(|x| String::from_utf8_lossy(&x.data).trim_end_matches('\0').to_string())
            .unwrap_or_default();
        if !edid.eq_ignore_ascii_case(&wanted) {
            return;
        }
        let names: Vec<String> = subs
            .iter()
            .filter(|x| x.sig == *b"SCVR")
            .map(|x| String::from_utf8_lossy(&x.data).trim_end_matches('\0').to_string())
            .collect();
        let slsd: Vec<&[u8]> = subs
            .iter()
            .filter(|x| x.sig == *b"SLSD")
            .map(|x| x.data.as_slice())
            .collect();
        println!("{edid}: {} SCVR, {} SLSD", names.len(), slsd.len());
        for i in 0..names.len().max(slsd.len()) {
            let name = names.get(i).cloned().unwrap_or_else(|| "<none>".to_string());
            match slsd.get(i) {
                Some(d) if d.len() >= 20 => println!(
                    "  {i:>3}  id {:>5}  flags {:02X}  len {:>2}  {name}",
                    u16::from_le_bytes([d[0], d[1]]),
                    d[16],
                    d.len()
                ),
                Some(d) => println!("  {i:>3}  SLSD of only {} bytes  {name}", d.len()),
                None => println!("  {i:>3}  no SLSD  {name}"),
            }
        }
    });
}
