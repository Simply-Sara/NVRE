//! The ALCH (`Ingestible`) editor reads and writes `ENIT` correctly.
//!
//! `ENIT` is 20 bytes on every one of the 189 ALCH records in
//! `FalloutNV.esm`, and the spec's layout is:
//!
//! ```text
//! value              i32    bytes 0..4
//! flags              u8     byte  4
//! unused             byte[3] bytes 5..8
//! Withdrawal Effect  formid bytes 8..12
//! Addiction Chance   f32    bytes 12..16
//! Sound - Consume    formid bytes 16..20
//! ```
//!
//! The editor had the last two the wrong way round: it read the Addiction
//! Chance out of the Withdrawal Effect's four bytes and the Withdrawal
//! Effect out of the Addiction Chance's four. 31 of the 189 records have
//! both fields non-zero, so the panel showed a number where a FormID was and
//! a FormID where a number was.
//!
//! The editor also rebuilt the flag byte from its three checkboxes, which
//! cleared bit 0 ("No Auto-Calc") on every record that set it — 180 of the
//! 189.

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
    let path = std::env::temp_dir().join(format!("nvre_alch_{name}.esp"));
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

/// An ALCH with both a Withdrawal Effect and an Addiction Chance:
/// 0x001673D6 carries withdrawal 0x0006698C and chance 0.1.
const ALCH_BOTH: u32 = 0x0016_73D6;

#[test]
fn the_withdrawal_effect_and_the_addiction_chance_are_not_swapped() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };

    let before = master_subs(&s, ALCH_BOTH);
    let enit = payload(&before, b"ENIT").expect("the record has an ENIT");
    assert_eq!(enit.len(), 20, "ENIT is 20 bytes on every ALCH record");

    let withdrawal = u32::from_le_bytes([enit[8], enit[9], enit[10], enit[11]]);
    let chance = f32::from_le_bytes([enit[12], enit[13], enit[14], enit[15]]);
    assert_ne!(withdrawal, 0, "the fixture has a Withdrawal Effect");
    assert_ne!(chance, 0.0, "the fixture has an Addiction Chance");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ALCH_BOTH, *b"ALCH");

    assert_eq!(
        state.alch_withdrawal,
        Some(withdrawal),
        "the Withdrawal Effect is the FormID in bytes 8..12"
    );
    assert_eq!(
        state.alch_addict_chance, chance,
        "the Addiction Chance is the float in bytes 12..16"
    );
}

#[test]
fn the_flag_byte_is_not_cleared_on_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };

    let before = master_subs(&s, ALCH_BOTH);
    let before_enit = payload(&before, b"ENIT").expect("the record has an ENIT");
    assert_ne!(
        before_enit[4] & 0x01,
        0,
        "the fixture sets bit 0 (No Auto-Calc), which the old editor cleared"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ALCH_BOTH, *b"ALCH");
    state.save_to_session(&mut s);

    let after = saved_subs(&s, ALCH_BOTH, "after");
    let after_enit = payload(&after, b"ENIT").expect("the record still has an ENIT");

    assert_eq!(
        after_enit, before_enit,
        "a save that changed nothing must reproduce the record's ENIT byte for byte"
    );
}
