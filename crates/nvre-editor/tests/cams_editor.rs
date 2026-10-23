//! The Camera Shot (CAMS) editor reads and writes every field of every
//! subrecord in fopdoc's CAMS spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, the model data collection, a `DATA` struct and
//! `MNAM`. The `DATA` struct holds three enums, a flag word and six floats.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 276 CAMS records. `DATA` is forty bytes on 272 of them and thirty-six
//!   on four, so the length is kept and a short record is not grown.
//! - `DocuCamBackLeft01HitC` (0x00175C3A): action 2 (Hit), location 0
//!   (Attacker), target 2 (Target), flags 7, a player time multiplier of 6.0,
//!   and the image space modifier `0x0007DC93`.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// DocuCamBackLeft01HitC (0x00175C3A): a forty-byte DATA.
const CAMS_HIT: u32 = 0x0017_5C3A;

/// DocuCamBackRight01HitB (0x00175C38): different time multipliers.
const CAMS_HIT_B: u32 = 0x0017_5C38;

#[test]
fn every_field_of_a_real_camera_shot_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CAMS_HIT, *b"CAMS");

    assert_eq!(state.edid, "DocuCamBackLeft01HitC");

    // --- DATA: the three enums ---
    assert_eq!(state.cams_action, 2, "DATA: Action (Hit)");
    assert_eq!(state.cams_location, 0, "DATA: Location (Attacker)");
    assert_eq!(state.cams_target, 2, "DATA: Target (Target)");

    // --- DATA: the flag word ---
    assert_eq!(state.cams_flags, 7, "DATA: Flags");
    assert_ne!(state.cams_flags & 0x01, 0, "0x01 Position Follows Location");
    assert_ne!(state.cams_flags & 0x02, 0, "0x02 Rotation Follows Target");
    assert_ne!(state.cams_flags & 0x04, 0, "0x04 Don't Follow Bone");
    assert_eq!(
        state.cams_flags & 0x08,
        0,
        "0x08 First Person Camera is clear"
    );

    // --- DATA: the six floats ---
    assert_eq!(
        state.cams_player_time_mult, 6.0,
        "DATA: Player Time Multiplier"
    );
    assert_eq!(
        state.cams_target_time_mult, 0.1,
        "DATA: Target Time Multiplier"
    );
    assert_eq!(
        state.cams_global_time_mult, 0.125,
        "DATA: Global Time Multiplier"
    );
    assert_eq!(state.cams_max_time, 2.0, "DATA: Max Time");
    assert_eq!(state.cams_min_time, 0.1, "DATA: Min Time");

    // --- MNAM: the image space modifier ---
    assert_eq!(
        state.cams_image_space_modifier,
        Some(0x0007_DC93),
        "MNAM: Image Space Modifier"
    );

    // --- The model data collection ---
    assert_eq!(
        state.cams_model.modl, "vatscameras\\docubackcamleft01.nif",
        "MODL: the model path"
    );
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CAMS_HIT, *b"CAMS");
    assert_eq!(state.cams_player_time_mult, 6.0);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CAMS_HIT_B, *b"CAMS");
    assert_eq!(state.edid, "DocuCamBackRight01HitB");
    assert_eq!(
        state.cams_player_time_mult, 1.25,
        "DATA: this record's own player time multiplier"
    );
    assert_eq!(
        state.cams_global_time_mult, 0.5,
        "DATA: this record's own global time multiplier"
    );
    assert_eq!(state.cams_max_time, 6.0, "DATA: this record's own max time");
}

#[test]
fn a_camera_shot_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = CAMS_HIT;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CAMS");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "cams_identity");

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
fn a_thirty_six_byte_data_is_not_grown_to_forty() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // Four of the game's 276 CAMS records carry a thirty-six-byte DATA. A
    // fixed forty-byte write would grow them.
    // Every record is loaded and saved, and the file is written once at the
    // end: each `Session::save()` rewrites the whole master, so saving once
    // per record would turn a 276-record probe into gigabytes of writes.
    let mut before: std::collections::BTreeMap<u32, usize> = Default::default();
    for fid in common::fids_of(&s, b"CAMS") {
        before.insert(fid, payload(&subs(&s, fid), b"DATA").len());
        let mut state = ObjectEditorState::default();
        state.load_from_session(&s, fid, *b"CAMS");
        state.save_to_session(&mut s);
    }

    let path = std::env::temp_dir().join("nvre_cams_datalen.esp");
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = nvre_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "all.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&path);

    let mut short = 0;
    let mut long = 0;
    let mut checked = 0;
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        if r.header.sig != *b"CAMS" {
            return;
        }
        let subs = r.subrecords().expect("subs");
        let len = subs
            .iter()
            .find(|s| s.sig == *b"DATA")
            .map(|s| s.data.len())
            .expect("a DATA");
        assert_eq!(
            len, before[&r.header.form_id],
            "{:08X}: DATA keeps its own length",
            r.header.form_id
        );
        if len == 36 {
            short += 1;
        } else {
            long += 1;
        }
        checked += 1;
    });
    assert_eq!(checked, 276, "every CAMS record came back");
    assert_eq!(short, 4, "four of the records carry a thirty-six-byte DATA");
    assert_eq!(long, 272, "272 of the records carry a forty-byte DATA");
}

#[test]
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = CAMS_HIT;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CAMS");

    // DATA: the three enums, the flag word and the six floats.
    state.cams_action = 0;
    state.cams_location = 1;
    state.cams_target = 0;
    state.cams_flags |= 0x08; // First Person Camera
    state.cams_player_time_mult = 1.5;
    state.cams_target_time_mult = 2.5;
    state.cams_global_time_mult = 3.5;
    state.cams_max_time = 4.5;
    state.cams_min_time = 5.5;
    state.cams_target_percent = 6.5;

    // MNAM: the image space modifier.
    state.cams_image_space_modifier = Some(0x0012_3456);

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "cams_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 40, "DATA keeps its forty bytes");
    let u32_at =
        |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    let f32_at =
        |at: usize| f32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    assert_eq!(u32_at(0), 0, "the edited action");
    assert_eq!(u32_at(4), 1, "the edited location");
    assert_eq!(u32_at(8), 0, "the edited target");
    assert_eq!(
        u32_at(12) & 0x08,
        0x08,
        "the edited First Person Camera flag"
    );
    assert_eq!(u32_at(12) & 0x01, 0x01, "the unnamed flags survive");
    assert_eq!(f32_at(16), 1.5, "the edited player time multiplier");
    assert_eq!(f32_at(20), 2.5, "the edited target time multiplier");
    assert_eq!(f32_at(24), 3.5, "the edited global time multiplier");
    assert_eq!(f32_at(28), 4.5, "the edited max time");
    assert_eq!(f32_at(32), 5.5, "the edited min time");
    assert_eq!(f32_at(36), 6.5, "the edited target percent");

    let mnam = payload(&after, b"MNAM");
    assert_eq!(
        u32::from_le_bytes([mnam[0], mnam[1], mnam[2], mnam[3]]),
        0x0012_3456,
        "the edited image space modifier"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, CAMS_HIT, *b"CAMS");
    let title = nvre_editor::object_editor::window_title(&s.schema, &state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Camera Shot - DocuCamBackLeft01HitC");
}

#[test]
fn every_camera_shot_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"CAMS", 276);
}
