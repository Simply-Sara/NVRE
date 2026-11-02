//! Probe: follow a placed ref's NAME (base FormID) to the base record and
//! its SCRV script, the chain a remote variable on a placed reference
//! resolves through.
//! ```text
//! NVRE_DATA=<Data> cargo run -p nvre-script --example probe_base2 -- <SCRO-name>
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
    // FormID -> (sig, edid, name-base, scrv, scri)
    let mut info: std::collections::HashMap<
        u32,
        (String, String, Option<u32>, Option<u32>, Option<u32>),
    > = Default::default();
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
        // NAME base form: the first subrecord after EDID with 4 bytes of
        // formid — for ACHR/REFR it is NAME; for NPC_ it is also NAME (the
        // base object's own name is a full-name string, but NPC_ NAME is a
        // zstring... measure, don't assume: print raw).
        let mut base: Option<u32> = None;
        let mut scrv: Option<u32> = None;
        let mut scri: Option<u32> = None;
        for s in subs.iter() {
            if s.sig == *b"NAME"
                && s.data.len() == 4
                && (sig == "ACHR" || sig == "REFR" || sig == "ACRE" || sig == "PGRD")
            {
                base = Some(u32::from_le_bytes([
                    s.data[0], s.data[1], s.data[2], s.data[3],
                ]));
            }
            if s.sig == *b"SCRV" && s.data.len() == 4 {
                scrv = Some(u32::from_le_bytes([
                    s.data[0], s.data[1], s.data[2], s.data[3],
                ]));
            }
            if s.sig == *b"SCRI" && s.data.len() == 4 {
                scri = Some(u32::from_le_bytes([
                    s.data[0], s.data[1], s.data[2], s.data[3],
                ]));
            }
        }
        info.insert(rec.header.form_id, (sig, edid, base, scrv, scri));
    });
    // find the wanted ref
    let fid = info
        .iter()
        .find(|(_, (_, e, _, _, _))| e.eq_ignore_ascii_case(&wanted))
        .map(|(f, _)| *f);
    let Some(fid) = fid else {
        println!("{wanted}: no record with that EDID");
        return;
    };
    let mut cur = fid;
    for depth in 0..6 {
        let Some((sig, edid, base, scrv, scri)) = info.get(&cur).cloned() else {
            println!("{cur:08X}: [not in this master]");
            break;
        };
        println!("{depth}: {cur:08X} {sig} edid {edid} base {base:?} scrv {scrv:?} scri {scri:?}");
        if let Some(b) = base {
            cur = b;
        } else {
            break;
        }
    }
}
