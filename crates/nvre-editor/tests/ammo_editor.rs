//! The AMMO (`Ammunition`) editor reads and writes `QNAM` correctly.
//!
//! `QNAM` is the ammo's **abbreviation**, a cstring. The editor used to read
//! it as a FormID, which turned "5mm" into a number, and then wrote those
//! four bytes back over the abbreviation — so opening an ammo record and
//! saving it replaced its abbreviation with garbage.
//!
//! The measured facts this test leans on came from `fnvtool esm survey` and
//! `fnvtool esm decode` over `FalloutNV.esm`, not from a fixture written from
//! memory:
//!
//! - 92 AMMO records; 48 carry a `QNAM` with 23 distinct values ("5mm",
//!   ".223", "Bag", "AP", "CS", "HM", "HP", "HV", "Inc.", "JFP", "JHP",
//!   "JSP", "M", "Bulk", "+P", ...).
//! - No `QNAM` value is a FormID: the shortest is 2 bytes, and 20 of the 48
//!   are 4 bytes or longer, which the old reader would have turned into a
//!   bogus FormID.
//! - The spec's `RCIL` (Ammo Effect) is the FormID, and `DAT2` carries the
//!   consumed ammo, so there is no FormID in `QNAM` at all.

use nvre_editor::object_editor::ObjectEditorState;
use nvre_editor::session::Session;

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

fn open_master() -> Option<Session> {
    let data = data_dir()?;
    Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}

/// The record's subrecords as they are in the master, before any edit.
fn master_subs(s: &Session, fid: u32) -> Vec<fnv_esm::Subrecord> {
    let rec = s.doc.record(fid).expect("the record is in the master");
    rec.subrecords().expect("subrecords parse")
}

/// The subrecords of the record, read from the file the editor actually wrote.
fn saved_subs(s: &Session, fid: u32, name: &str) -> Vec<fnv_esm::Subrecord> {
    let path = std::env::temp_dir().join(format!("nvre_ammo_{name}.esp"));
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), format!("{name}.esp"))
        .expect("parse");
    let mut found = None;
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
        if r.header.form_id == fid {
            found = Some(r.clone());
        }
    });
    let _ = std::fs::remove_file(&path);
    found
        .expect("the record survives the save")
        .subrecords()
        .expect("subrecords parse")
}

fn payload(subs: &[fnv_esm::Subrecord], sig: &[u8; 4]) -> Option<Vec<u8>> {
    subs.iter().find(|s| &s.sig == sig).map(|s| s.data.clone())
}

/// An AMMO whose `QNAM` is ".223" (0x00160C41). The abbreviation is five
/// bytes with the NUL terminator, so it is the length class the old reader
/// broke: it read the first four bytes as a FormID and wrote those four back,
/// dropping the terminator and leaving the engine to read past the end of
/// the string.
const AMMO_QNAM: u32 = 0x0016_0C41;

#[test]
fn the_abbreviation_is_read_as_a_string_not_a_formid() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };

    let before = master_subs(&s, AMMO_QNAM);
    let raw = payload(&before, b"QNAM").expect("the record has a QNAM");
    let text = String::from_utf8_lossy(&raw)
        .trim_end_matches('\0')
        .to_string();

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, AMMO_QNAM, *b"AMMO");

    assert_eq!(
        state.ammo_abbrev, text,
        "the editor shows the abbreviation the record carries"
    );
}

#[test]
fn a_save_that_changed_nothing_keeps_the_abbreviation() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };

    let before = master_subs(&s, AMMO_QNAM);
    let before_qnam = payload(&before, b"QNAM").expect("the record has a QNAM");
    assert_eq!(
        before_qnam.len(),
        5,
        "the fixture is a five-byte abbreviation, the length class the old \
         FormID reader corrupted"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, AMMO_QNAM, *b"AMMO");
    state.save_to_session(&mut s);

    let after = saved_subs(&s, AMMO_QNAM, "after");
    let after_qnam = payload(&after, b"QNAM").expect("the record still has a QNAM");

    assert_eq!(
        after_qnam, before_qnam,
        "a save that changed nothing must not rewrite the record's abbreviation \
         as a FormID's four bytes"
    );
}
