//! Proof that the object editor's flat, per-type state loses bytes on records
//! whose subrecord is shorter than the editor's fixed-size rebuild.
//!
//! The ARMO (`Armor`) `DNAM` subrecord is the clearest case:
//!
//! - 368 of the 389 ARMO records in `FalloutNV.esm` carry a 12-byte `DNAM`.
//! - The other 21 carry a 4-byte `DNAM` — just the `AR` int16 and two unused
//!   bytes, with no `DT` and no `Flags`.
//!
//! `save_armo` rebuilds `DNAM` as a fresh 12-byte buffer every time, so
//! opening one of the 21 short records and saving it turns a 4-byte subrecord
//! into a 12-byte one. That is a silent data change: the record's own bytes
//! are replaced by the editor's reconstruction, and the `DT` and
//! `Modulates Voice` fields gain values the record never had.
//!
//! The measured facts this test leans on came from `fnvtool esm survey` over
//! `FalloutNV.esm`, not from a fixture written from memory.

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
fn master_subs(s: &Session, fid: u32) -> Vec<nvre_esm::Subrecord> {
    let rec = s.doc.record(fid).expect("the record is in the master");
    rec.subrecords().expect("subrecords parse")
}

/// The subrecords of the record, read from the file the editor actually wrote.
fn saved_subs(s: &Session, fid: u32, name: &str) -> Vec<nvre_esm::Subrecord> {
    let path = std::env::temp_dir().join(format!("nvre_armo_{name}.esp"));
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = nvre_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), format!("{name}.esp"))
        .expect("parse");
    let mut found = None;
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
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

fn payload(subs: &[nvre_esm::Subrecord], sig: &[u8; 4]) -> Option<Vec<u8>> {
    subs.iter().find(|s| &s.sig == sig).map(|s| s.data.clone())
}

/// An ARMO with a 12-byte `DNAM`: ArmorCombatHelmetReinforcedmark2
/// (0x0015EF67), `dt` 5.0.
const ARMO_FULL_DNAM: u32 = 0x0015_EF67;

/// An ARMO with a 4-byte `DNAM`: one of the 21 records that carry only the
/// `AR` int16 and two unused bytes. 0x0013F297 is one of them.
const ARMO_SHORT_DNAM: u32 = 0x0013_F297;

#[test]
fn a_short_dnam_is_not_grown_to_twelve_bytes() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };

    // The fixture must really be a short one, or the test proves nothing.
    let before = master_subs(&s, ARMO_SHORT_DNAM);
    let before_dnam = payload(&before, b"DNAM").expect("the record has a DNAM");
    assert_eq!(
        before_dnam.len(),
        4,
        "the fixture must be one of the 21 ARMO records with a 4-byte DNAM, \
         not a 12-byte one"
    );

    // Open it in the editor and save it without changing anything.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ARMO_SHORT_DNAM, *b"ARMO");
    state.save_to_session(&mut s);

    let after = saved_subs(&s, ARMO_SHORT_DNAM, "after");
    let after_dnam = payload(&after, b"DNAM").expect("the record still has a DNAM");

    assert_eq!(
        after_dnam.len(),
        4,
        "a save that changed nothing must not grow the record's 4-byte DNAM \
         into a 12-byte one; the editor rebuilt the subrecord from its own \
         fields and dropped the record's own bytes"
    );
}

#[test]
fn a_full_dnam_survives_a_save_unchanged() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };

    let before = master_subs(&s, ARMO_FULL_DNAM);
    let before_dnam = payload(&before, b"DNAM").expect("the record has a DNAM");
    assert_eq!(
        before_dnam.len(),
        12,
        "the fixture is a 12-byte DNAM record"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ARMO_FULL_DNAM, *b"ARMO");
    state.save_to_session(&mut s);

    let after = saved_subs(&s, ARMO_FULL_DNAM, "after_full");
    let after_dnam = payload(&after, b"DNAM").expect("the record still has a DNAM");

    assert_eq!(
        after_dnam, before_dnam,
        "a save that changed nothing must reproduce the record's DNAM byte for byte"
    );
}

#[test]
fn a_field_read_conditionally_does_not_keep_the_last_records_value() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };

    // Open the record that has a 12-byte DNAM first: its `DT` is 5.0.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ARMO_FULL_DNAM, *b"ARMO");
    assert_eq!(state.armo_dt, 5.0, "the first record's DT is 5.0");

    // Now open one of the 21 records whose DNAM is four bytes and has no DT
    // at all. The editor must not show the previous record's 5.0.
    state.load_from_session(&s, ARMO_SHORT_DNAM, *b"ARMO");
    assert_eq!(
        state.armo_dt, 0.0,
        "the second record has no DT of its own, so the editor must not \
         still be showing the first record's 5.0"
    );
}
