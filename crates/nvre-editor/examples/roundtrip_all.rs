//! Round-trip every record of every type against the shipped master, in one
//! pass.
//!
//! For each record type in the Objects window, every record is loaded into
//! the editor and saved back with no edit. The saved record must be
//! identical to the original, subrecord for subrecord, in order. A type
//! that adds or drops a subrecord, or reorders one, fails here.
//!
//! The check runs against the document's own record list, with the queued
//! edits applied, so it does not write the 245 MB master to disk.

use std::collections::BTreeMap;

use nvre_editor::object_editor::ObjectEditorState;
use nvre_editor::session::Session;

/// The record types the Objects window shows, and how many records of each
/// the master has.
const TYPES: &[(&str, &[u8; 4], usize)] = &[
    ("ACTI", b"ACTI", 1143),
    ("ADDN", b"ADDN", 37),
    ("ALCH", b"ALCH", 189),
    ("ALOC", b"ALOC", 89),
    ("AMEF", b"AMEF", 54),
    ("AMMO", b"AMMO", 92),
    ("ANIO", b"ANIO", 152),
    ("ARMA", b"ARMA", 131),
    ("ARMO", b"ARMO", 389),
    ("ASPC", b"ASPC", 113),
    ("AVIF", b"AVIF", 64),
    ("BOOK", b"BOOK", 27),
    ("BPTD", b"BPTD", 49),
    ("CAMS", b"CAMS", 276),
    ("CCRD", b"CCRD", 270),
    ("CDCK", b"CDCK", 13),
    ("CHAL", b"CHAL", 105),
    ("CHIP", b"CHIP", 5),
    ("CLAS", b"CLAS", 74),
    ("CLMT", b"CLMT", 31),
    ("CMNY", b"CMNY", 6),
    ("CONT", b"CONT", 2478),
    ("CPTH", b"CPTH", 418),
    ("CREA", b"CREA", 1578),
    ("CSNO", b"CSNO", 5),
    ("CSTY", b"CSTY", 84),
    ("DEBR", b"DEBR", 6),
    ("DEHY", b"DEHY", 5),
    ("DOBJ", b"DOBJ", 1),
    ("DOOR", b"DOOR", 320),
    ("ECZN", b"ECZN", 17),
    ("EFSH", b"EFSH", 35),
    ("ENCH", b"ENCH", 145),
    ("EXPL", b"EXPL", 154),
    ("EYES", b"EYES", 12),
    ("FACT", b"FACT", 682),
    ("FLST", b"FLST", 464),
    ("FURN", b"FURN", 234),
    ("GLOB", b"GLOB", 218),
    ("GMST", b"GMST", 648),
    ("GRAS", b"GRAS", 24),
    ("HAIR", b"HAIR", 67),
    ("HDPT", b"HDPT", 61),
    ("HUNG", b"HUNG", 5),
    ("IDLE", b"IDLE", 1597),
    ("IDLM", b"IDLM", 211),
    ("IMAD", b"IMAD", 215),
    ("IMGS", b"IMGS", 67),
    ("IMOD", b"IMOD", 50),
    ("INGR", b"INGR", 1),
    ("IPCT", b"IPCT", 125),
    ("IPDS", b"IPDS", 60),
    ("KEYM", b"KEYM", 283),
    ("LGTM", b"LGTM", 31),
    ("LIGH", b"LIGH", 501),
    ("LSCR", b"LSCR", 208),
    ("LSCT", b"LSCT", 1),
    ("LVLC", b"LVLC", 343),
    ("LVLI", b"LVLI", 2738),
    ("LVLN", b"LVLN", 365),
    ("MESG", b"MESG", 1144),
    ("MGEF", b"MGEF", 289),
    ("MICN", b"MICN", 12),
    ("MISC", b"MISC", 507),
    ("MSET", b"MSET", 140),
    ("MSTT", b"MSTT", 241),
    ("MUSC", b"MUSC", 20),
    ("NOTE", b"NOTE", 894),
    ("NPC_", b"NPC_", 3816),
    ("PACK", b"PACK", 4163),
    ("PERK", b"PERK", 176),
    ("PROJ", b"PROJ", 95),
    ("PWAT", b"PWAT", 29),
    ("QUST", b"QUST", 436),
    ("RACE", b"RACE", 22),
    ("RADS", b"RADS", 5),
    ("RCCT", b"RCCT", 10),
    ("RCPE", b"RCPE", 105),
    ("REGN", b"REGN", 276),
    ("REPU", b"REPU", 13),
    ("RGDL", b"RGDL", 38),
    ("SCOL", b"SCOL", 98),
    ("SCPT", b"SCPT", 2576),
    ("SLPD", b"SLPD", 5),
    ("SOUN", b"SOUN", 3190),
    ("SPEL", b"SPEL", 270),
    ("STAT", b"STAT", 6785),
    ("TACT", b"TACT", 87),
    ("TERM", b"TERM", 344),
    ("TREE", b"TREE", 3),
    ("TXST", b"TXST", 493),
    ("VTYP", b"VTYP", 100),
    ("WATR", b"WATR", 78),
    ("WEAP", b"WEAP", 261),
    ("WTHR", b"WTHR", 63),
];

/// One subrecord: its signature and its payload.
type Sub = ([u8; 4], Vec<u8>);

/// The subrecords of a record, in file order, as signature and payload.
fn shape(subs: &[fnv_esm::Subrecord]) -> Vec<Sub> {
    subs.iter().map(|s| (s.sig, s.data.clone())).collect()
}

fn main() {
    let data = match std::env::var("NVRE_DATA") {
        Ok(d) => d,
        Err(_) => {
            eprintln!("NVRE_DATA is not set");
            std::process::exit(2);
        }
    };
    let schema = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv");
    let mut session = Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(schema),
        vec!["FalloutNV.esm".to_string()],
    )
    .expect("open the master");

    // The original shape of every record of every type, keyed by FormID.
    // A record whose payload is compressed cannot be split into subrecords,
    // so it is skipped.
    let mut before: BTreeMap<u32, Vec<Sub>> = BTreeMap::new();
    for (fid, rec) in session.doc.records() {
        let Ok(subs) = rec.subrecords() else {
            continue;
        };
        before.insert(fid, shape(&subs));
    }

    // Every record of every type goes through a load and a save with no
    // edit. The edits queue up in the document.
    for (name, sig, expected) in TYPES {
        let sig: &[u8; 4] = sig;
        let fids: Vec<u32> = session
            .doc
            .records()
            .into_iter()
            .filter(|(f, r)| &r.header.sig == sig && before.contains_key(f))
            .map(|(f, _)| f)
            .collect();
        if fids.len() != *expected {
            eprintln!(
                "{name}: the master has {} readable records, expected {expected}",
                fids.len()
            );
            std::process::exit(1);
        }
        for fid in &fids {
            let mut state = ObjectEditorState::default();
            state.load_from_session(&session, *fid, *sig);
            state.save_to_session(&mut session);
        }
    }

    // Now read the records back with the queued edits applied, and compare.
    let mut failures: Vec<String> = Vec::new();
    for (name, sig, expected) in TYPES {
        let sig: &[u8; 4] = sig;
        let fids: Vec<u32> = session
            .doc
            .records()
            .into_iter()
            .filter(|(f, r)| &r.header.sig == sig && before.contains_key(f))
            .map(|(f, _)| f)
            .collect();
        let mut bad = 0usize;
        let mut first_problem = String::new();
        for fid in &fids {
            let b = &before[fid];
            let mut rec = session.doc.record(*fid).expect("record").clone();
            if let Some(edits) = session.doc.edits_for(*fid) {
                nvre_edit::document::apply_edits_pub(&mut rec, edits);
            }
            let Ok(subs) = rec.subrecords() else {
                continue;
            };
            let a = shape(&subs);
            if a.len() != b.len() {
                if bad == 0 {
                    first_problem = format!(
                        "{fid:08X}: {} subrecords come back, expected {}",
                        a.len(),
                        b.len()
                    );
                }
                bad += 1;
                continue;
            }
            for (i, (bs, bd)) in b.iter().enumerate() {
                if a[i] != (*bs, bd.clone()) {
                    if bad == 0 {
                        first_problem = format!(
                            "{fid:08X}: subrecord {i} ({}) changes",
                            fnv_util::tag_str(bs)
                        );
                    }
                    bad += 1;
                    break;
                }
            }
        }
        if bad == 0 {
            println!("{name:5} ok    {expected} records round-trip");
        } else {
            println!("{name:5} FAIL  {bad}/{expected}  {first_problem}");
            failures.push(format!("{name}: {bad} records differ"));
        }
    }

    if failures.is_empty() {
        println!("\nall types round-trip byte for byte");
    } else {
        println!("\n{} failing types:", failures.len());
        for f in &failures {
            println!("  {f}");
        }
        std::process::exit(1);
    }
}
