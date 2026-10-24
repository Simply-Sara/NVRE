//! The Hair (HAIR) editor reads and writes every field of every subrecord in
//! fopdoc's HAIR spec, and keeps the record intact through a save.
//!
//! The spec lists `EDID`, `FULL`, the model data collection (`MODL`, `MODB`,
//! `MODT`, `MODS`, `MODD`), `ICON` and a one-byte `DATA` flag word. What is
//! checked here is that the values are the game's own values, and that a save
//! reproduces the record byte for byte when nothing was changed.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with
//! `fnvtool esm survey` and `fnvtool esm decode`, not against a fixture
//! written from memory. The measured facts the tests lean on:
//!
//! - 67 HAIR records, every one of them `EDID FULL MODL MODT ICON DATA` in
//!   that order, with a one-byte `DATA` and a 96-byte `MODT`.
//! - The flag word takes four values: 2 (6 records), 3 (30), 4 (3) and 5 (28).
//!   No record sets "Fixed" (bit 3), and no record sets "Playable" alone.
//! - No HAIR record carries `MODB`, `MODS` or `MODD`.

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
    let path = std::env::temp_dir().join(format!("nvre_hair_{name}.esp"));
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

/// The signature sequence of a record, in file order.
fn sig_order(subs: &[nvre_esm::Subrecord]) -> Vec<String> {
    subs.iter()
        .map(|s| nvre_util::tag_str(&s.sig).to_string())
        .collect()
}

/// HairFemaleCherry (0x0005239D): flags 2 (Not Male), a male model and a
/// child icon.
const HAIR_FEMALE_CHERRY: u32 = 0x0005_239D;

/// HairCurly (0x0005DC7F): flags 5 (Playable and Not Female).
const HAIR_CURLY: u32 = 0x0005_DC7F;

/// HairWavyOld (0x00044527): flags 4 (Not Female) — the only flag value that
/// no other record carries.
const HAIR_WAVY_OLD: u32 = 0x0004_4527;

/// HairAfricanAmericanF (0x0005DC78): flags 3 (Playable and Not Male).
const HAIR_AFRICAN_AMERICAN: u32 = 0x0005_DC78;

#[test]
fn every_field_of_a_real_hair_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HAIR_FEMALE_CHERRY, *b"HAIR");

    assert_eq!(state.edid, "HairFemaleCherry");
    assert_eq!(state.full_name, "Pretty Puff");

    // --- ICON ---
    assert_eq!(
        state.hair_icon, "Characters\\Hair\\HairChildMCurly.dds",
        "the hair's texture"
    );

    // --- DATA: the flag word ---
    assert_eq!(state.hair_flags, 2, "Flags: Not Male");
    assert_eq!(state.hair_flags & 0x01, 0, "0x01 Playable is clear");
    assert_ne!(state.hair_flags & 0x02, 0, "0x02 Not Male");
    assert_eq!(state.hair_flags & 0x04, 0, "0x04 Not Female is clear");
    assert_eq!(state.hair_flags & 0x08, 0, "0x08 Fixed is clear");

    // --- The model data collection ---
    let model = &state.hair_model;
    assert_eq!(
        model.modl, "Characters\\Hair\\HairFemaleCherry.NIF",
        "MODL: the model's path"
    );
    assert_eq!(model.modt.len(), 96, "MODT: 96 bytes of texture hashes");
    assert_eq!(model.modt.len() % 24, 0, "MODT is a whole number of hashes");
    assert_eq!(model.modb, None, "MODB: this record has none");
    assert_eq!(model.mods.len(), 0, "MODS: this record has none");
    assert_eq!(model.modd, None, "MODD: this record has none");

    // The header's Model Path field is the collection's MODL.
    assert_eq!(state.model, model.modl, "the header shows the same path");
}

#[test]
fn every_flag_value_the_game_uses_is_read() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The four flag values the master's HAIR records use, and the record that
    // carries each. The game uses 2, 3, 4 and 5.
    let cases = [
        (HAIR_FEMALE_CHERRY, 2u8, "Not Male"),
        (HAIR_CURLY, 5, "Playable and Not Female"),
        (HAIR_AFRICAN_AMERICAN, 3, "Playable and Not Male"),
        (HAIR_WAVY_OLD, 4, "Not Female"),
    ];
    for (fid, flags, what) in cases {
        let mut state = ObjectEditorState::default();
        state.load_from_session(&s, fid, *b"HAIR");
        assert_eq!(state.hair_flags, flags, "{what}");
    }

    // The bits, one at a time.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HAIR_CURLY, *b"HAIR");
    assert_ne!(state.hair_flags & 0x01, 0, "0x01 Playable");
    assert_eq!(state.hair_flags & 0x02, 0, "0x02 Not Male is clear");
    assert_ne!(state.hair_flags & 0x04, 0, "0x04 Not Female");
    assert_eq!(state.hair_flags & 0x08, 0, "0x08 Fixed");
}

#[test]
fn a_hair_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = HAIR_FEMALE_CHERRY;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HAIR");
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
    // And the DATA is still the one byte it was.
    assert_eq!(payload(&after, b"DATA").len(), 1, "DATA is one byte");
}

#[test]
fn the_data_flag_word_is_not_rewritten_as_a_longer_one() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The old editor had no HAIR arm at all, so HAIR fell through to
    // save_misc, which wrote an eight-byte DATA the record does not have.
    // Every one of the game's 67 HAIR records has a one-byte DATA.
    let fid = HAIR_CURLY;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HAIR");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "datalen");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 1, "DATA is still one byte");
    assert_eq!(data[0], 5, "and holds the record's own flag value");
}

#[test]
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = HAIR_FEMALE_CHERRY;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HAIR");

    // DATA: the flag word, with a bit the spec does not name.
    state.hair_flags = 0x0F;

    // ICON: the hair's texture.
    state.hair_icon = "Characters\\Hair\\HairFemaleCherryAlt.dds".to_string();

    // MODL: the model's path, through the header's Model Path field.
    state.model = "Characters\\Hair\\HairFemaleCherryNew.NIF".to_string();

    // MODT: the texture hashes are opaque file hashes, so the editor keeps
    // them; the test checks they survive an edit of another field.
    let modt_before = state.hair_model.modt.clone();

    // MODB and MODD: neither is in this record, so the editor must not
    // invent them.
    assert_eq!(state.hair_model.modb, None, "no MODB to begin with");
    assert_eq!(state.hair_model.modd, None, "no MODD to begin with");

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "edited");

    // DATA.
    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 1, "DATA is one byte");
    assert_eq!(data[0], 0x0F, "the edited flag word");

    // ICON.
    let icon = payload(&after, b"ICON");
    assert_eq!(
        String::from_utf8_lossy(&icon).trim_end_matches('\0'),
        "Characters\\Hair\\HairFemaleCherryAlt.dds",
        "the edited texture"
    );

    // MODL.
    let modl = payload(&after, b"MODL");
    assert_eq!(
        String::from_utf8_lossy(&modl).trim_end_matches('\0'),
        "Characters\\Hair\\HairFemaleCherryNew.NIF",
        "the edited model path"
    );

    // MODT: the hashes are preserved byte for byte.
    assert_eq!(
        payload(&after, b"MODT"),
        modt_before,
        "the texture hashes are copied through, not rewritten"
    );

    // And the subrecords the record never had are still absent.
    assert!(
        !after.iter().any(|s| s.sig == *b"MODB"),
        "no MODB is invented"
    );
    assert!(
        !after.iter().any(|s| s.sig == *b"MODS"),
        "no MODS is invented"
    );
    assert!(
        !after.iter().any(|s| s.sig == *b"MODD"),
        "no MODD is invented"
    );
}

#[test]
fn the_texture_hashes_are_carried_through_a_no_edit_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // MODT holds four 24-byte texture hashes per HAIR record. A texture hash
    // is a file hash the editor cannot compute, so it must be copied through
    // exactly.
    let fid = HAIR_AFRICAN_AMERICAN;
    let before = payload(&subs(&s, fid), b"MODT");
    assert_eq!(before.len(), 96, "the record's MODT is 96 bytes");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HAIR");
    state.hair_flags = 0x0F; // edit something else entirely
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "modt"), b"MODT");
    assert_eq!(after, before, "the texture hashes are byte-identical");
}

#[test]
fn a_hair_can_be_made_playable_and_not_female() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The "Fixed" bit (0x08) is not set by any of the game's HAIR records, so
    // it is proved here by writing it: the flag word is kept whole, and the
    // named bits are edited inside it.
    let fid = HAIR_WAVY_OLD;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HAIR");
    assert_eq!(state.hair_flags, 4, "Not Female only");

    state.hair_flags |= 0x08; // Fixed
    state.hair_flags |= 0x01; // Playable
    state.save_to_session(&mut s);

    let data = payload(&saved_subs(&s, fid, "fixed"), b"DATA");
    assert_eq!(data[0], 0x0D, "Playable and Fixed and Not Female");
    assert_ne!(data[0] & 0x08, 0, "0x08 Fixed");
    assert_ne!(data[0] & 0x01, 0, "0x01 Playable");
    assert_ne!(data[0] & 0x04, 0, "0x04 Not Female");
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HAIR_FEMALE_CHERRY, *b"HAIR");
    let title = nvre_editor::object_editor::window_title(
        &s.schema,
        &state.sig,
        &state.edid,
        &state.full_name,
    );
    assert_eq!(title, "Hair - HairFemaleCherry (Pretty Puff)");
}

#[test]
fn every_hair_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The strongest statement: every one of the game's 67 HAIR records, loaded
    // and saved with no edit, comes back byte for byte.
    let fids: Vec<u32> = s
        .doc
        .records()
        .into_iter()
        .filter(|(_, r)| r.header.sig == *b"HAIR")
        .map(|(f, _)| f)
        .collect();
    assert_eq!(fids.len(), 67, "the master has 67 HAIR records");

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
        state.load_from_session(&s, *fid, *b"HAIR");
        state.save_to_session(&mut s);
    }

    let path = std::env::temp_dir().join("nvre_hair_all.esp");
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = nvre_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "all.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&path);

    let mut after = std::collections::BTreeMap::new();
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        if r.header.sig == *b"HAIR" {
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
