//! The QUST (`Quest`) editor keeps the record's `DATA` at the length the
//! record itself has.
//!
//! 434 of the 436 QUST records in `FalloutNV.esm` carry an eight-byte
//! `DATA` — a flags byte, a priority byte, four unused bytes and a float32
//! delay. The other two carry only two bytes: the flags and the priority,
//! with no delay at all. They are `RadioTenpiny` (0x0001E5CA) and
//! `GenericSupMutBehemoth` (0x0002242A).
//!
//! The editor rebuilt `DATA` as a fresh eight-byte buffer every time, so
//! opening one of those two records and saving it grew the subrecord and
//! gave the quest a delay of zero it never had.

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
    let path = std::env::temp_dir().join(format!("nvre_qust_{name}.esp"));
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

/// One of the two QUST records with a two-byte `DATA`.
const QUST_SHORT: u32 = 0x0002_242A;

#[test]
fn a_two_byte_data_is_not_grown_to_eight_bytes() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };

    let before = master_subs(&s, QUST_SHORT);
    let before_data = payload(&before, b"DATA").expect("the record has a DATA");
    assert_eq!(
        before_data.len(),
        2,
        "the fixture must be one of the two QUST records with a two-byte DATA"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, QUST_SHORT, *b"QUST");
    state.save_to_session(&mut s);

    let after = saved_subs(&s, QUST_SHORT, "after");
    let after_data = payload(&after, b"DATA").expect("the record still has a DATA");

    assert_eq!(
        after_data.len(),
        2,
        "a save that changed nothing must not grow the record's two-byte DATA \
         into an eight-byte one"
    );
    assert_eq!(
        after_data, before_data,
        "and the bytes it does carry must be the record's own"
    );
}
