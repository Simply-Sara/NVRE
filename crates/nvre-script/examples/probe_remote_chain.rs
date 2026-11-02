//! Probe: follow an SCRO FormID to the script it runs.
//!
//! The remote name in the source (`VNPCFollowers`) is an SCRO entry's EDID;
//! the bytecode reads the variable table of the script that FormID's record
//! runs. This probe follows the chain for the names the parity failures
//! spell: SCRO FormID -> the record -> its SCRV (a script FormID) -> that
//! script's EDID -> whether an SCPT with that EDID exists.
//!
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_remote_chain
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

    // Pass one: every record's signature, EDID, SCRV (FormID), and — for
    // SCPTs — whether it has SCTX.
    struct Rec {
        sig: String,
        edid: String,
        scrv_fid: Option<u32>,
        has_scpt_source: bool,
    }
    let mut by_fid: std::collections::HashMap<u32, Rec> = Default::default();
    let mut scpt_by_edid: std::collections::HashMap<String, u32> = Default::default();
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
        let scrv_fid = subs
            .iter()
            .find(|x| x.sig == *b"SCRV")
            .filter(|x| x.data.len() == 4)
            .map(|x| u32::from_le_bytes([x.data[0], x.data[1], x.data[2], x.data[3]]));
        let has_scpt_source =
            sig == "SCPT" && subs.iter().any(|x| x.sig == *b"SCTX" && !x.data.is_empty());
        if sig == "SCPT" {
            scpt_by_edid.insert(edid.clone(), rec.header.form_id);
        }
        by_fid.insert(
            rec.header.form_id,
            Rec {
                sig,
                edid,
                scrv_fid,
                has_scpt_source,
            },
        );
    });

    let names = [
        "VNPCFollowers",
        "VHDLegionBattle",
        "VHDBattleController",
        "VStreetFluffNPC",
        "VDialogueHiddenValley",
        "VMS18",
        "BH01",
        "VMS38",
        "VMS01",
        "VFactionSquad",
        "VFreeformNovac",
    ];
    for name in names {
        // The SCRO entry: the FormID whose EDID is the name.
        let fid = by_fid
            .iter()
            .find(|(_, r)| r.edid.eq_ignore_ascii_case(name))
            .map(|(f, _)| *f);
        let Some(fid) = fid else {
            println!("{name}: no record with that EDID");
            continue;
        };
        let rec = &by_fid[&fid];
        // Follow SCRV.
        let mut line = format!("{name}: FormID {fid:08X} sig {} edid {}", rec.sig, rec.edid);
        if let Some(script_fid) = rec.scrv_fid {
            if let Some(script_rec) = by_fid.get(&script_fid) {
                line += &format!(
                    " -> SCRV {script_fid:08X} sig {} edid {} scpt_source {}",
                    script_rec.sig, script_rec.edid, script_rec.has_scpt_source
                );
                // Is there an SCPT whose EDID is the script's name?
                if scpt_by_edid.contains_key(&script_rec.edid) {
                    line += " [table exists]";
                }
            } else {
                line += &format!(" -> SCRV {script_fid:08X} [no record]");
            }
        } else {
            line += " [no SCRV]";
            // Maybe the record itself IS a script (an SCPT with this EDID)?
            if scpt_by_edid.contains_key(name) {
                line += " [but an SCPT carries this name]";
            }
        }
        println!("{line}");
    }
}
