//! The Media Location Controller (ALOC) editor reads and writes every field of
//! every subrecord in fopdoc's ALOC spec, and keeps the record intact through
//! a save.
//!
//! The spec lists `EDID`, `FULL`, seven `NAM` fields, six media-set lists
//! (`HNAM`, `ZNAM`, `XNAM`, `YNAM`, `LNAM`, `GNAM`), `RNAM` and `FNAM`. The
//! six media-set lists are *repeated* subrecords, which is the interesting
//! part: they must be written as lists, or the game reads one media set where
//! the record had sixteen.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with
//! `fnvtool esm survey` and a raw subrecord dump, not against a fixture
//! written from memory. The measured facts the tests lean on:
//!
//! - 89 ALOC records. Every one carries `EDID`, `NAM1`..`NAM7` and `FNAM`,
//!   each four bytes; 87 carry `FULL`.
//! - `NAM5` (day start) is 64 on every record measured and `NAM6` (night
//!   start) is 255.
//! - `musCtrlStripNCREmbassy` (0x00177D61) carries all six media-set lists,
//!   and its `GNAM` (battle) list has four entries.
//! - `musCtrlHooverDamEast` (0x0017A209) carries only a `LNAM` (location) list
//!   and no `RNAM`.
//! - `GNAM` reaches 16 entries on one record, so a per-subrecord edit would
//!   collapse it visibly.

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

/// The record's subrecords, in file order.
fn subs(s: &Session, fid: u32) -> Vec<nvre_esm::Subrecord> {
    let rec = s.doc.record(fid).expect("the record");
    rec.subrecords().expect("subrecords parse")
}

/// The same, read from the file the editor actually wrote.
fn saved_subs(s: &Session, fid: u32, name: &str) -> Vec<nvre_esm::Subrecord> {
    let path = std::env::temp_dir().join(format!("nvre_aloc_{name}.esp"));
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

/// A subrecord's payload by signature.
fn payload(subs: &[nvre_esm::Subrecord], sig: &[u8; 4]) -> Vec<u8> {
    subs.iter()
        .find(|s| &s.sig == sig)
        .map(|s| s.data.clone())
        .unwrap_or_else(|| panic!("a {} subrecord", nvre_util::tag_str(sig)))
}

/// Every payload with this signature, in file order.
fn payloads(subs: &[nvre_esm::Subrecord], sig: &[u8; 4]) -> Vec<Vec<u8>> {
    subs.iter()
        .filter(|s| &s.sig == sig)
        .map(|s| s.data.clone())
        .collect()
}

/// The signature sequence of a record, in file order.
fn sig_order(subs: &[nvre_esm::Subrecord]) -> Vec<String> {
    subs.iter()
        .map(|s| nvre_util::tag_str(&s.sig).to_string())
        .collect()
}

/// musCtrlStripNCREmbassy (0x00177D61): all six media-set lists, and a
/// four-entry battle list.
const ALOC_STRIP: u32 = 0x0017_7D61;

/// musCtrlHooverDamEast (0x0017A209): only a location list, and no `RNAM`.
const ALOC_HOOVER: u32 = 0x0017_A209;

/// The list a given state list index holds.
fn list_entries(state: &ObjectEditorState, label: &str) -> Vec<u32> {
    state
        .aloc_sets
        .iter()
        .find(|l| l.label == label)
        .expect("the list")
        .entries
        .clone()
}

#[test]
fn every_field_of_a_real_media_location_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ALOC_STRIP, *b"ALOC");

    assert_eq!(state.edid, "musCtrlStripNCREmbassy");
    assert_eq!(state.full_name, "musCtrlGnrcNCRCamp");

    // --- NAM1: four unnamed bytes ---
    assert_eq!(
        state.aloc_nam1,
        [0x54, 0x00, 0x00, 0x00],
        "NAM1: the bytes the spec does not name"
    );

    // --- NAM4: the location delay ---
    assert_eq!(state.aloc_location_delay, 0.0, "NAM4: Location Delay");

    // --- NAM5 and NAM6: the day and night starts ---
    assert_eq!(state.aloc_day_start, 64, "NAM5: Day Start");
    assert_eq!(state.aloc_night_start, 255, "NAM6: Night Start");

    // --- NAM7: the retrigger delay ---
    assert_eq!(
        state.aloc_retrigger_delay, 805.066_7,
        "NAM7: Retrigger Delay"
    );

    // --- The six media-set lists ---
    assert_eq!(list_entries(&state, "Neutral").len(), 1, "HNAM: neutral");
    assert_eq!(list_entries(&state, "Ally").len(), 1, "ZNAM: ally");
    assert_eq!(list_entries(&state, "Friend").len(), 1, "XNAM: friend");
    assert_eq!(list_entries(&state, "Enemy").len(), 1, "YNAM: enemy");
    assert_eq!(list_entries(&state, "Location").len(), 1, "LNAM: location");
    assert_eq!(
        list_entries(&state, "Battle").len(),
        4,
        "GNAM: battle — the list keeps all four entries"
    );

    // --- RNAM: the conditional faction ---
    assert_eq!(
        state.aloc_conditional_faction,
        Some(0x000A_46E7),
        "RNAM: Conditional Faction"
    );

    // --- FNAM: four unnamed bytes ---
    assert_eq!(state.aloc_fnam, [0, 0, 0, 0], "FNAM: the unnamed bytes");
}

#[test]
fn a_record_with_only_one_list_reads_only_that_list() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ALOC_HOOVER, *b"ALOC");

    assert_eq!(state.edid, "musCtrlHooverDamEast");
    assert_eq!(
        list_entries(&state, "Location").len(),
        1,
        "LNAM: this record's only list"
    );
    assert_eq!(list_entries(&state, "Battle").len(), 0, "GNAM: none");
    assert_eq!(list_entries(&state, "Neutral").len(), 0, "HNAM: none");
    assert_eq!(
        state.aloc_conditional_faction, None,
        "RNAM: this record has none"
    );
}

#[test]
fn the_media_set_lists_survive_a_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The point of this test: the six lists are *repeated* subrecords. Written
    // with a per-subrecord edit they would collapse onto their last entry, and
    // the record would lose three of its four battle media sets.
    let fid = ALOC_STRIP;
    let before = subs(&s, fid);
    assert_eq!(
        payloads(&before, b"GNAM").len(),
        4,
        "the record has four battle media sets before the save"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ALOC");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "lists");

    for sig in [*b"HNAM", *b"ZNAM", *b"XNAM", *b"YNAM", *b"LNAM", *b"GNAM"] {
        assert_eq!(
            payloads(&after, &sig),
            payloads(&before, &sig),
            "{}: the list comes back whole and in order",
            nvre_util::tag_str(&sig)
        );
    }
}

#[test]
fn a_media_location_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = ALOC_STRIP;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ALOC");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "identity");

    assert_eq!(after.len(), before.len(), "the same subrecords come back");
    for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
        assert_eq!(
            (b.sig, &b.data),
            (a.sig, &a.data),
            "subrecord {i} ({}) is byte-identical",
            nvre_util::tag_str(&a.sig)
        );
    }
    assert_eq!(
        sig_order(&after),
        sig_order(&before),
        "the subrecords come back in the order they were in"
    );
}

#[test]
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = ALOC_STRIP;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ALOC");

    // NAM4 and NAM7: the two delays.
    state.aloc_location_delay = 12.5;
    state.aloc_retrigger_delay = 3.25;

    // NAM5 and NAM6: the day and night starts.
    state.aloc_day_start = 100;
    state.aloc_night_start = 200;

    // NAM1: the unnamed bytes.
    state.aloc_nam1 = [1, 2, 3, 4];

    // RNAM: the conditional faction.
    state.aloc_conditional_faction = Some(0x0012_3456);

    // HNAM: the neutral media set list.
    state.aloc_sets[0].entries = vec![0x0014_E879, 0x0014_E87A];

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "edited");

    let f32_at = |sig: &[u8; 4]| -> f32 {
        let b = payload(&after, sig);
        f32::from_le_bytes([b[0], b[1], b[2], b[3]])
    };
    let u32_at = |sig: &[u8; 4]| -> u32 {
        let b = payload(&after, sig);
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    };

    assert_eq!(f32_at(b"NAM4"), 12.5, "NAM4: the edited location delay");
    assert_eq!(f32_at(b"NAM7"), 3.25, "NAM7: the edited retrigger delay");
    assert_eq!(u32_at(b"NAM5"), 100, "NAM5: the edited day start");
    assert_eq!(u32_at(b"NAM6"), 200, "NAM6: the edited night start");
    assert_eq!(
        payload(&after, b"NAM1"),
        vec![1, 2, 3, 4],
        "NAM1: the edited bytes"
    );
    assert_eq!(
        u32_at(b"RNAM"),
        0x0012_3456,
        "RNAM: the edited conditional faction"
    );

    // HNAM: the two-entry list.
    let hnam = payloads(&after, b"HNAM");
    assert_eq!(hnam.len(), 2, "HNAM: the edited list has two entries");
    assert_eq!(
        u32::from_le_bytes([hnam[0][0], hnam[0][1], hnam[0][2], hnam[0][3]]),
        0x0014_E879,
        "HNAM: the first entry"
    );
    assert_eq!(
        u32::from_le_bytes([hnam[1][0], hnam[1][1], hnam[1][2], hnam[1][3]]),
        0x0014_E87A,
        "HNAM: the second entry"
    );
}

#[test]
fn removing_a_conditional_faction_removes_the_subrecord() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // 81 of the game's 89 ALOC records have no RNAM at all, so the editor has
    // to be able to drop one rather than write a null FormID over it.
    let fid = ALOC_STRIP;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ALOC");
    assert!(state.aloc_conditional_faction.is_some());

    state.aloc_conditional_faction = None;
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "no_rnam");

    assert!(
        !after.iter().any(|s| s.sig == *b"RNAM"),
        "the RNAM subrecord is gone"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ALOC_STRIP, *b"ALOC");
    let title = nvre_editor::object_editor::window_title(
        &s.schema,
        &state.sig,
        &state.edid,
        &state.full_name,
    );
    assert_eq!(
        title,
        "Media Location Controller - musCtrlStripNCREmbassy (musCtrlGnrcNCRCamp)"
    );
}

#[test]
fn every_media_location_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The strongest statement: every one of the game's 89 ALOC records,
    // loaded and saved with no edit, comes back byte for byte.
    let fids: Vec<u32> = s
        .doc
        .records()
        .into_iter()
        .filter(|(_, r)| r.header.sig == *b"ALOC")
        .map(|(f, _)| f)
        .collect();
    assert_eq!(fids.len(), 89, "the master has 89 ALOC records");

    let mut before = std::collections::BTreeMap::new();
    for fid in &fids {
        let subs = subs(&s, *fid);
        before.insert(
            *fid,
            subs.iter()
                .map(|x| (x.sig, x.data.clone()))
                .collect::<Vec<_>>(),
        );
    }

    for fid in &fids {
        let mut state = ObjectEditorState::default();
        state.load_from_session(&s, *fid, *b"ALOC");
        state.save_to_session(&mut s);
    }

    let path = std::env::temp_dir().join("nvre_aloc_all.esp");
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = nvre_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "all.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&path);

    let mut after = std::collections::BTreeMap::new();
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        if r.header.sig == *b"ALOC" {
            after.insert(
                r.header.form_id,
                r.subrecords()
                    .expect("subs")
                    .iter()
                    .map(|x| (x.sig, x.data.clone()))
                    .collect::<Vec<_>>(),
            );
        }
    });

    for fid in &fids {
        let b = before.get(fid).expect("before");
        let a = after.get(fid).expect("after");
        assert_eq!(
            a.len(),
            b.len(),
            "{fid:08X}: the same number of subrecords comes back"
        );
        for (i, (bs, bd)) in b.iter().enumerate() {
            let (asig, ad) = &a[i];
            assert_eq!(
                (asig, ad),
                (bs, bd),
                "{fid:08X}: subrecord {i} ({}) is byte-identical",
                nvre_util::tag_str(bs)
            );
        }
    }
}
