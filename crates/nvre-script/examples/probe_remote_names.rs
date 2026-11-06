//! Probe: how does a source's remote-script name map to the script whose
//! variable table the bytecode reads?
//!
//! The parity harness keyed remote tables by SCPT EDID, but sources name
//! quests (`VNPCFollowers`, `BH01`), not scripts. The chain this probe
//! verifies: the name resolves in the record's SCRO list to a FormID; that
//! FormID's base form carries an `SCRV` (or the name IS a script EDID
//! directly); the SCRV names the script whose SCPT holds the table.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_remote_names
//! ```

use bytes::Bytes;
use nvre_esm::raw::visit_nodes;
use nvre_esm::Plugin;

fn main() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("set NVRE_DATA");
        return;
    };
    let path = std::path::Path::new(&data).join("FalloutNV.esm");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("cannot read master");
        return;
    };
    let Ok(plugin) = Plugin::parse_bytes(Bytes::from(bytes), "FalloutNV.esm".to_string()) else {
        eprintln!("cannot parse master");
        return;
    };

    // FormID -> EDID for every record; and every SCPT's own variable table
    // keyed by its EDID. Also collect, per record, whether it carries SCRV
    // and what it names.
    let mut formid_to_edid: std::collections::HashMap<u32, String> = Default::default();
    let mut script_tables: std::collections::HashMap<String, Vec<(String, u16, bool)>> =
        Default::default();
    // EDID -> SCRV-named script EDID, for records that carry SCRV.
    let mut edid_to_script: std::collections::HashMap<String, String> = Default::default();

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
        formid_to_edid.insert(rec.header.form_id, edid.clone());

        // SCRV: the script this record runs. Its data is a FormID (4 bytes)
        // or a zstring name, depending on the record — print both shapes.
        if let Some(scrv) = subs.iter().find(|x| x.sig == *b"SCRV") {
            if scrv.data.len() == 4 {
                let fid =
                    u32::from_le_bytes([scrv.data[0], scrv.data[1], scrv.data[2], scrv.data[3]]);
                edid_to_script.insert(edid.clone(), format!("__fid_{fid:08X}"));
            } else {
                let name = String::from_utf8_lossy(&scrv.data)
                    .trim_end_matches('\0')
                    .to_string();
                edid_to_script.insert(edid.clone(), name);
            }
        }

        if rec.header.sig == *b"SCPT" {
            let names: Vec<String> = subs
                .iter()
                .filter(|x| x.sig == *b"SCVR")
                .map(|x| {
                    String::from_utf8_lossy(&x.data)
                        .trim_end_matches('\0')
                        .to_string()
                })
                .collect();
            let slcds: Vec<&[u8]> = subs
                .iter()
                .filter(|x| x.sig == *b"SLSD" && x.data.len() >= 20)
                .map(|x| x.data.as_slice())
                .collect();
            if names.len() == slcds.len() && !names.is_empty() {
                let table: Vec<(String, u16, bool)> = names
                    .into_iter()
                    .zip(slcds)
                    .map(|(n, d)| (n, u16::from_le_bytes([d[0], d[1]]), d[16] == 1))
                    .collect();
                script_tables.insert(edid, table);
            }
        }
    });

    println!("SCPT tables: {}", script_tables.len());
    println!("records carrying SCRV: {}", edid_to_script.len());
    // SCRV shapes: FormID or name?
    let fid_form = edid_to_script
        .values()
        .filter(|v| v.starts_with("__fid_"))
        .count();
    println!(
        "SCRV as FormID: {fid_form}, as name: {}",
        edid_to_script.len() - fid_form
    );

    // The failing sources' remote names. For each, resolve: SCRO FormID ->
    // base EDID -> SCRV -> script EDID -> table exists?
    let remote_names = [
        "VNPCFollowers",
        "VHDLegionBattle",
        "VHDBattleController",
        "VStreetFluffNPC",
        "VDialogueHiddenValley",
        "VMS18",
        "BH01",
        "VMS38",
        "VMS01",
    ];
    println!();
    for name in remote_names {
        // Find the FormID whose EDID is this name.
        let fid = formid_to_edid
            .iter()
            .find(|(_, e)| e.eq_ignore_ascii_case(name))
            .map(|(f, _)| *f);
        match fid {
            Some(f) => {
                let script = edid_to_script.get(name).cloned();
                let script_fid_edid = script
                    .as_deref()
                    .and_then(|s| s.strip_prefix("__fid_"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .and_then(|f| formid_to_edid.get(&f).cloned());
                let table_known = script
                    .as_ref()
                    .map(|s| script_tables.contains_key(s))
                    .unwrap_or(false);
                let direct = script_tables.contains_key(name);
                println!(
                    "{name}: SCRO FormID {f:08X}, SCRV -> {script:?} (resolves to {script_fid_edid:?}, table known: {table_known}), direct table: {direct}"
                );
            }
            None => println!("{name}: no FormID with that EDID"),
        }
    }
}
