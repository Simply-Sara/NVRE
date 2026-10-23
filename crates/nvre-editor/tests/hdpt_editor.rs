//! The Head Part (HDPT) editor reads and writes every field of every
//! subrecord in fopdoc's HDPT spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, `FULL`, the model data collection (`MODL`, `MODB`,
//! `MODT`, `MODS`, `MODD`), a one-byte `DATA` flag word and a repeated `HNAM`
//! naming other head parts. What is checked here is that the values are the
//! game's own values, and that a save reproduces the record byte for byte
//! when nothing was changed.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with
//! `fnvtool esm survey`, `fnvtool esm inventory` and `fnvtool esm decode`, not
//! against a fixture written from memory. The measured facts the tests lean
//! on:
//!
//! - 61 HDPT records, in six distinct subrecord orders:
//!   `EDID FULL MODL MODT MODS DATA` (23), `EDID FULL MODL MODT DATA` (17),
//!   `EDID FULL DATA HNAM HNAM` (12), `EDID FULL MODL MODS DATA` (3),
//!   `EDID FULL MODL DATA` (3) and `EDID FULL DATA HNAM HNAM HNAM` (3).
//! - Every record's `DATA` is one byte. 47 records set "Playable" (0x01) and
//!   14 records do not, so the word takes exactly the two values 0 and 1.
//! - `HNAM` is always 4 bytes and always a FormID to another HDPT record:
//!   12 records carry two, 3 records carry three, and 46 carry none.
//! - `MODS` is a counted list of alternate textures. The count is 1 in 23
//!   records and 2 in 3 records. Every entry is a 3D name, a `TXST` FormID
//!   and a 3D index, and the index is -1 or 0 in every record.
//! - `MODT` is 96 bytes (four hashes) in 39 records and 48 bytes (two hashes)
//!   in one; the other 21 records carry no `MODT` at all.
//! - No HDPT record carries `MODB` or `MODD`.

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
    let path = std::env::temp_dir().join(format!("nvre_hdpt_{name}.esp"));
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

/// Every subrecord with the given signature, in file order.
fn payloads(subs: &[nvre_esm::Subrecord], sig: &[u8; 4]) -> Vec<Vec<u8>> {
    subs.iter()
        .filter(|s| &s.sig == sig)
        .map(|s| s.data.clone())
        .collect()
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

/// A FormID out of a 4-byte payload.
fn fid_of(b: &[u8]) -> u32 {
    assert_eq!(b.len(), 4, "a FormID payload is four bytes");
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// BeardFull (0x0002E379): playable, a model, and a 96-byte MODT. The
/// richest of the "plain" records — it has no `MODS` and no `HNAM`.
const HDPT_BEARD_FULL: u32 = 0x0002_E379;

/// Eyepatch (0x00073D56): not playable, and the only record whose `MODT` is
/// 48 bytes rather than 96.
const HDPT_EYEPATCH: u32 = 0x0007_3D56;

/// BeardMustacheThinOld (0x0007B489): not playable, a model, and one
/// alternate texture in `MODS`.
const HDPT_BEARD_MUSTACHE_THIN_OLD: u32 = 0x0007_B489;

/// BeardMustacheFull (0x0009F920): playable, a model, a 96-byte `MODT` and
/// two alternate textures.
const HDPT_BEARD_MUSTACHE_FULL: u32 = 0x0009_F920;

/// BeardMix02c (0x000BCBF9): playable, no model at all, and two extra
/// parts.
const HDPT_BEARD_MIX_02C: u32 = 0x000B_CBF9;

/// BeardMix6c (0x000BCC01): playable, no model, and three extra parts — the
/// only records with three.
const HDPT_BEARD_MIX_6C: u32 = 0x000B_CC01;

/// `BeardFullT` (0x0009F925), the `TXST` the game's alternate textures point
/// at.
const TXST_BEARD_FULL_T: u32 = 0x0009_F925;

/// `BeardThinT` (0x0009F926), the `TXST` `BeardMustacheThinOld` points at.
const TXST_BEARD_THIN_T: u32 = 0x0009_F926;

/// `BeardLincolnT` (0x0009F928), the other `TXST` the game uses.
const TXST_BEARD_LINCOLN_T: u32 = 0x0009_F928;

/// `BeardGoatee` (0x00066FD5), an extra part of `BeardMix02c`.
const HDPT_BEARD_GOATEE: u32 = 0x0006_6FD5;

/// `BeardMustacheCurly` (0x00066FCF), the other extra part of `BeardMix02c`.
const HDPT_BEARD_MUSTACHE_CURLY: u32 = 0x0006_6FCF;

/// `BeardSoulPatch` (0x00066FD3), a head part used to edit the extra-parts
/// list.
const HDPT_BEARD_SOUL_PATCH: u32 = 0x0006_6FD3;

#[test]
fn every_field_of_a_real_head_part_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HDPT_BEARD_FULL, *b"HDPT");

    assert_eq!(state.edid, "BeardFull");
    assert_eq!(state.full_name, "Survivalist");

    // --- DATA: the flag word ---
    assert_eq!(state.hdpt_flags, 1, "Flags: Playable");
    assert_ne!(state.hdpt_flags & 0x01, 0, "0x01 Playable");

    // --- The model data collection ---
    let model = &state.hdpt_model;
    assert_eq!(
        model.modl, "Characters\\Hair\\BeardFull.NIF",
        "MODL: the model's path"
    );
    assert_eq!(model.modt.len(), 96, "MODT: 96 bytes of texture hashes");
    assert_eq!(model.modt.len() % 24, 0, "MODT is a whole number of hashes");
    assert_eq!(model.modb, None, "MODB: this record has none");
    assert_eq!(model.mods.len(), 0, "MODS: this record has none");
    assert_eq!(model.modd, None, "MODD: this record has none");

    // The header's Model Path field is the collection's MODL.
    assert_eq!(state.model, model.modl, "the header shows the same path");

    // --- HNAM: the extra parts ---
    assert_eq!(
        state.hdpt_extra_parts.len(),
        0,
        "HNAM: this record has none"
    );
}

#[test]
fn the_extra_parts_are_read_in_file_order() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // BeardMix02c carries two extra parts; BeardMix6c carries three. The
    // subrecord repeats, so reading it out of a one-value-per-signature map
    // would collapse them onto the last one.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HDPT_BEARD_MIX_02C, *b"HDPT");
    assert_eq!(
        state.hdpt_extra_parts,
        vec![HDPT_BEARD_GOATEE, HDPT_BEARD_MUSTACHE_CURLY],
        "the two extra parts, in the order the record carries them"
    );

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HDPT_BEARD_MIX_6C, *b"HDPT");
    assert_eq!(
        state.hdpt_extra_parts,
        vec![0x000B_22BD, 0x000B_22C0, 0x0006_6FCE],
        "the three extra parts, in the order the record carries them"
    );
}

#[test]
fn the_alternate_textures_are_read() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // BeardMustacheFull carries two alternate textures: the same 3D name
    // twice, pointing at two different TXST records, with 3D indices -1
    // and 0.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HDPT_BEARD_MUSTACHE_FULL, *b"HDPT");
    let mods = &state.hdpt_model.mods;
    assert_eq!(mods.len(), 2, "MODS: two alternate textures");
    assert_eq!(mods[0].name, "BeardMustache:0", "the first 3D name");
    assert_eq!(mods[0].texture, TXST_BEARD_FULL_T, "the first TXST");
    assert_eq!(mods[0].index, 0, "the first 3D index");
    assert_eq!(mods[1].name, "BeardMustache:0", "the second 3D name");
    assert_eq!(mods[1].texture, TXST_BEARD_LINCOLN_T, "the second TXST");
    assert_eq!(mods[1].index, -1, "the second 3D index");

    // BeardMustacheThinOld carries one.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HDPT_BEARD_MUSTACHE_THIN_OLD, *b"HDPT");
    let mods = &state.hdpt_model.mods;
    assert_eq!(mods.len(), 1, "MODS: one alternate texture");
    assert_eq!(mods[0].name, "BeardMustacheOld:0", "the 3D name");
    assert_eq!(mods[0].texture, TXST_BEARD_THIN_T, "the TXST");
    assert_eq!(mods[0].index, 0, "the 3D index");
}

#[test]
fn both_values_of_the_playable_flag_are_read() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The game's 61 HDPT records carry exactly two flag values: 1 (Playable)
    // on 47 of them and 0 on the other 14.
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HDPT_BEARD_FULL, *b"HDPT");
    assert_eq!(state.hdpt_flags, 1, "Playable");
    assert_ne!(state.hdpt_flags & 0x01, 0, "0x01 Playable");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HDPT_EYEPATCH, *b"HDPT");
    assert_eq!(state.hdpt_flags, 0, "not playable");
    assert_eq!(state.hdpt_flags & 0x01, 0, "0x01 Playable is clear");
}

#[test]
fn a_head_part_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = HDPT_BEARD_FULL;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HDPT");
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
    // The editor had no HDPT arm at all, so HDPT fell through to save_misc,
    // which wrote an eight-byte DATA the record does not have. Every one of
    // the game's 61 HDPT records has a one-byte DATA.
    let fid = HDPT_BEARD_FULL;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HDPT");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "datalen");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 1, "DATA is still one byte");
    assert_eq!(data[0], 1, "and holds the record's own flag value");
}

#[test]
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = HDPT_BEARD_FULL;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HDPT");

    // DATA: the flag word, with a bit the spec does not name.
    state.hdpt_flags = 0x0F;

    // MODL: the model's path, through the header's Model Path field.
    state.model = "Characters\\Hair\\BeardFullNew.NIF".to_string();

    // MODT: the texture hashes are opaque file hashes, so the editor keeps
    // them; the test checks they survive an edit of another field.
    let modt_before = state.hdpt_model.modt.clone();

    // MODB and MODD: neither is in this record, so the editor must not
    // invent them.
    assert_eq!(state.hdpt_model.modb, None, "no MODB to begin with");
    assert_eq!(state.hdpt_model.modd, None, "no MODD to begin with");

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "edited");

    // DATA.
    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 1, "DATA is one byte");
    assert_eq!(data[0], 0x0F, "the edited flag word");

    // MODL.
    let modl = payload(&after, b"MODL");
    assert_eq!(
        String::from_utf8_lossy(&modl).trim_end_matches('\0'),
        "Characters\\Hair\\BeardFullNew.NIF",
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
fn the_extra_parts_can_be_edited_added_and_removed() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = HDPT_BEARD_MIX_02C;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HDPT");
    assert_eq!(
        state.hdpt_extra_parts,
        vec![HDPT_BEARD_GOATEE, HDPT_BEARD_MUSTACHE_CURLY],
        "the record's own two extra parts"
    );

    // Add one, remove one, and swap the other.
    state.hdpt_extra_parts.push(HDPT_BEARD_SOUL_PATCH);
    state.hdpt_extra_parts.remove(0);
    state.hdpt_extra_parts[0] = TXST_BEARD_FULL_T;

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "hnam");

    let hnam = payloads(&after, b"HNAM");
    assert_eq!(hnam.len(), 2, "HNAM: two extra parts come back");
    assert_eq!(fid_of(&hnam[0]), TXST_BEARD_FULL_T, "the edited extra part");
    assert_eq!(
        fid_of(&hnam[1]),
        HDPT_BEARD_SOUL_PATCH,
        "the added extra part"
    );
}

#[test]
fn a_record_with_no_extra_parts_gains_none() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // BeardFull has no HNAM at all. The editor writes the list it holds,
    // which is empty, so no HNAM is invented.
    let fid = HDPT_BEARD_FULL;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HDPT");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "nohnam");
    assert!(
        !after.iter().any(|s| s.sig == *b"HNAM"),
        "no HNAM is invented"
    );
}

#[test]
fn the_alternate_textures_can_be_edited() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = HDPT_BEARD_MUSTACHE_THIN_OLD;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HDPT");
    assert_eq!(
        state.hdpt_model.mods.len(),
        1,
        "MODS: one alternate texture"
    );

    state.hdpt_model.mods[0].name = "BeardMustacheOld:1".to_string();
    state.hdpt_model.mods[0].texture = TXST_BEARD_LINCOLN_T;
    state.hdpt_model.mods[0].index = 3;

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "mods");

    let mods = payload(&after, b"MODS");
    // count, name length, name, TXST, index. "BeardMustacheOld:1" is the
    // same 18 characters as the name the record carried.
    assert_eq!(mods.len(), 4 + 4 + 18 + 4 + 4, "MODS: the edited payload");
    assert_eq!(
        u32::from_le_bytes([mods[0], mods[1], mods[2], mods[3]]),
        1,
        "one entry"
    );
    assert_eq!(
        u32::from_le_bytes([mods[4], mods[5], mods[6], mods[7]]) as usize,
        18,
        "the name's length"
    );
    assert_eq!(
        String::from_utf8_lossy(&mods[8..26]),
        "BeardMustacheOld:1",
        "the edited 3D name"
    );
    assert_eq!(
        fid_of(&mods[26..30]),
        TXST_BEARD_LINCOLN_T,
        "the edited TXST"
    );
    assert_eq!(
        i32::from_le_bytes([mods[30], mods[31], mods[32], mods[33]]),
        3,
        "the edited 3D index"
    );
}

#[test]
fn the_texture_hashes_are_carried_through_a_no_edit_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // MODT holds 24-byte texture hashes the editor cannot compute, so they
    // must be copied through exactly. Eyepatch's is 48 bytes — two hashes —
    // which is the only HDPT record whose MODT is not 96 bytes.
    let fid = HDPT_EYEPATCH;
    let before = payload(&subs(&s, fid), b"MODT");
    assert_eq!(before.len(), 48, "the record's MODT is 48 bytes");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HDPT");
    state.hdpt_flags = 0x0F; // edit something else entirely
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "modt"), b"MODT");
    assert_eq!(after, before, "the texture hashes are byte-identical");
}

#[test]
fn a_head_part_can_be_made_unplayable() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The bit the spec does not name (0x02) is proved here by writing it:
    // the flag word is kept whole, and the named bit is edited inside it.
    let fid = HDPT_BEARD_FULL;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"HDPT");
    assert_eq!(state.hdpt_flags, 1, "Playable");

    state.hdpt_flags &= !0x01; // clear Playable
    state.hdpt_flags |= 0x02; // a bit the spec does not name
    state.save_to_session(&mut s);

    let data = payload(&saved_subs(&s, fid, "unplayable"), b"DATA");
    assert_eq!(data[0], 0x02, "the unnamed bit, and Playable cleared");
    assert_eq!(data[0] & 0x01, 0, "0x01 Playable is clear");
    assert_ne!(data[0] & 0x02, 0, "0x02 is the bit that was written");
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, HDPT_BEARD_FULL, *b"HDPT");
    let title = nvre_editor::object_editor::window_title(&s.schema, &state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Head Part - BeardFull (Survivalist)");
}

#[test]
fn every_head_part_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The strongest statement: every one of the game's 61 HDPT records,
    // loaded and saved with no edit, comes back byte for byte.
    let fids: Vec<u32> = s
        .doc
        .records()
        .into_iter()
        .filter(|(_, r)| r.header.sig == *b"HDPT")
        .map(|(f, _)| f)
        .collect();
    assert_eq!(fids.len(), 61, "the master has 61 HDPT records");

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

    // Every edit is queued into one save: each `Session::save()` rewrites
    // the whole master, so saving once per record would be 61 rewrites of
    // 245 MB.
    for fid in &fids {
        let mut state = ObjectEditorState::default();
        state.load_from_session(&s, *fid, *b"HDPT");
        state.save_to_session(&mut s);
    }

    let path = std::env::temp_dir().join("nvre_hdpt_all.esp");
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = nvre_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "all.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&path);

    let mut after = std::collections::BTreeMap::new();
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        if r.header.sig == *b"HDPT" {
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

    assert_eq!(after.len(), 61, "all 61 records survive the save");
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
