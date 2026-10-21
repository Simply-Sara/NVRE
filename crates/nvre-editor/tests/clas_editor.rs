//! The Class (CLAS) editor reads and writes every field of every subrecord in
//! fopdoc's CLAS spec, and keeps the record intact through a save.
//!
//! `DATA` is a fixed 28-byte struct — four int32 tag skills, the u32 flag word,
//! the u32 service word, int8 teaches, u8 max training level, and two unused
//! bytes — and `ATTR` is the seven SPECIAL attributes. Both are checked against
//! the game's own values, and a save is read back through the same reader the
//! editor uses.

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
fn subs(s: &Session, fid: u32) -> Vec<fnv_esm::Subrecord> {
    let rec = s.doc.record(fid).expect("the record");
    rec.subrecords().expect("subrecords parse")
}

/// The same, read from the file the editor actually wrote.
fn saved_subs(s: &Session, fid: u32, name: &str) -> Vec<fnv_esm::Subrecord> {
    let path = std::env::temp_dir().join(format!("nvre_clas_{name}.esp"));
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

/// The `DATA` payload of a record, from the document or from a saved file.
fn data_bytes(subs: &[fnv_esm::Subrecord]) -> Vec<u8> {
    subs.iter()
        .find(|s| s.sig == *b"DATA")
        .map(|s| s.data.clone())
        .expect("a DATA subrecord")
}

/// The `ATTR` payload of a record.
fn attr_bytes(subs: &[fnv_esm::Subrecord]) -> Vec<u8> {
    subs.iter()
        .find(|s| s.sig == *b"ATTR")
        .map(|s| s.data.clone())
        .expect("an ATTR subrecord")
}

#[test]
fn every_field_of_a_real_class_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // LegionaryBadAss: the class the test fixture already used, and one whose
    // tag skills, attributes and teaches value all differ from each other, so a
    // wrong offset cannot pass by coincidence.
    let fid = 0x0017_A636;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CLAS");

    assert_eq!(state.edid, "LegionaryBadAss");
    assert_eq!(state.full_name, "Legate", "FULL Name");

    // DATA's four tag skills, in order. The spec links these to the Actor Values
    // page, so the values are the actor-value enum: Melee Weapons is 38 and Guns
    // is 41, not 6 and 9.
    assert_eq!(
        state.clas_tag_skills,
        [38, 41, 45, -1],
        "the four Tag Skill int32s"
    );

    // DATA's flag word: this class is neither Playable nor Guard.
    assert!(!state.clas_playable, "Playable is clear");
    assert!(!state.clas_guard, "Guard is clear");
    assert_eq!(state.clas_flags, 0, "the whole flag word is zero");
    // DATA's service word: a non-vendor class buys and sells nothing.
    assert_eq!(state.clas_services, 0, "Buys/Sells and Services");
    assert_eq!(state.clas_teaches, 0, "Teaches: 0 = Barter");
    assert_eq!(state.clas_max_train, 0, "Maximum Training Level");

    // ATTR's seven attributes, in the spec's order.
    assert_eq!(
        state.clas_attributes,
        [9, 8, 8, 7, 6, 7, 7],
        "Strength through Luck"
    );

    // DESC is required by the spec and every class in the masters carries an
    // empty one, so an empty string is the value to load.
    assert_eq!(state.clas_desc, "", "DESC Description");
}

#[test]
fn a_vendor_class_reports_its_services() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // VendorWeapons ("Gun Merchant") is the class with the richest service word
    // in the masters: Weapons, Armor, Alcohol and Repair, so the flag bits are
    // exercised rather than all-zero.
    let fid = 0x0001_9462;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CLAS");

    assert_eq!(state.edid, "VendorWeapons");
    assert_eq!(state.clas_services, 131_079);
    assert_ne!(state.clas_services & 0x0000_0001, 0, "Weapons");
    assert_ne!(state.clas_services & 0x0000_0002, 0, "Armor");
    assert_ne!(state.clas_services & 0x0000_0004, 0, "Alcohol");
    assert_ne!(state.clas_services & 0x0002_0000, 0, "Repair");
    // And the bits that are clear stay clear, so the editor does not invent one.
    assert_eq!(state.clas_services & 0x0000_0040, 0, "Stimpaks is clear");
    assert_eq!(state.clas_services & 0x0000_4000, 0, "Training is clear");
    assert_eq!(
        state.clas_tag_skills,
        [32, 41, 34, -1],
        "Barter, Guns, Energy Weapons, None"
    );
    assert_eq!(state.clas_attributes, [4, 5, 4, 7, 5, 6, 5]);
}

#[test]
fn a_playable_class_reports_its_flags() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // VaultDweller: the player's own class, and one of the few with Playable set.
    let fid = 0x0000_0801;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CLAS");

    assert_eq!(state.edid, "VaultDweller");
    assert!(state.clas_playable, "Playable is set");
    assert!(!state.clas_guard, "Guard is clear");
    // 29 is 0b11101: Playable plus bits 2, 3 and 4, which fopdoc does not name.
    // The editor keeps the word whole so the unnamed bits survive an edit.
    assert_eq!(
        state.clas_flags, 29,
        "the whole flag word, unnamed bits included"
    );
    assert_eq!(state.clas_tag_skills, [41, 40, 45, -1]);
    assert_eq!(state.clas_attributes, [5, 5, 4, 5, 5, 5, 5]);
}

#[test]
fn an_unnamed_flag_bit_survives_an_edit() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // VaultDweller carries flag bits the spec does not name. Editing the Playable
    // checkbox must not clear them: the flag word is edited in place, not
    // rebuilt from the two named bits.
    let fid = 0x0000_0801;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CLAS");
    assert_eq!(state.clas_flags, 29);

    state.clas_playable = false;
    state.clas_guard = true;
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "unnamed_bits");
    let d = data_bytes(&after);
    let written = u32::from_le_bytes([d[16], d[17], d[18], d[19]]);
    assert_eq!(written, 30, "Guard on, Playable off, bits 2-4 kept");
    assert_eq!(written & 1, 0, "Playable is clear");
    assert_ne!(written & 2, 0, "Guard is set");
    assert_eq!(
        written & 0x1C,
        0x1C,
        "the three bits fopdoc does not name are untouched"
    );
}

#[test]
fn a_class_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = 0x0017_A636;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CLAS");
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
    // And the two the editor owns are the sizes the spec says they are.
    assert_eq!(data_bytes(&after).len(), 28, "DATA is 28 bytes");
    assert_eq!(attr_bytes(&after).len(), 7, "ATTR is 7 bytes");
}

#[test]
fn edited_fields_and_a_changed_description_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = 0x0017_A636;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CLAS");

    state.clas_tag_skills = [1, 2, 3, 4];
    state.clas_playable = true;
    state.clas_guard = true;
    state.clas_services = 0x0000_0043;
    state.clas_teaches = 11; // Speech, in the Skills enum
    state.clas_max_train = 42;
    state.clas_attributes = [1, 2, 3, 4, 5, 6, 7];
    state.clas_desc = "A legate of Caesar's Legion.".to_string();
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "edited");
    let d = data_bytes(&after);
    // The four tag skills, as int32s in order.
    assert_eq!(&d[0..4], &1i32.to_le_bytes(), "Tag Skill 1");
    assert_eq!(&d[4..8], &2i32.to_le_bytes(), "Tag Skill 2");
    assert_eq!(&d[8..12], &3i32.to_le_bytes(), "Tag Skill 3");
    assert_eq!(&d[12..16], &4i32.to_le_bytes(), "Tag Skill 4");
    // The flag word, then the service word. Editing the two named bits leaves
    // every unnamed bit exactly as it was.
    assert_eq!(
        u32::from_le_bytes([d[16], d[17], d[18], d[19]]),
        3,
        "Playable | Guard"
    );
    assert_eq!(
        u32::from_le_bytes([d[20], d[21], d[22], d[23]]),
        0x43,
        "Weapons | Armor | Stimpaks"
    );
    assert_eq!(d[24], 11, "Teaches: 11 = Speech in the Skills enum");
    assert_eq!(d[25], 42, "Maximum Training Level");
    // The struct's two unused bytes stay zero.
    assert_eq!(&d[26..28], &[0, 0], "Unused");

    // The seven attributes, in order.
    assert_eq!(attr_bytes(&after), vec![1, 2, 3, 4, 5, 6, 7]);

    // The description, NUL-terminated as a cstring.
    let desc = after
        .iter()
        .find(|s| s.sig == *b"DESC")
        .map(|s| s.data.clone())
        .expect("a DESC subrecord");
    assert_eq!(desc, b"A legate of Caesar's Legion.\0".to_vec());
}
