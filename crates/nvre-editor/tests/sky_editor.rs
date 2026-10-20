//! The Idle Animation (IDLM), Image Space (IMGS), Lighting (LGTM), Load
//! Screen (LSCR), Load Screen Type (LSCT), Mission Icon (MICN) and Music
//! (MUSC) editors read and write every field of every subrecord in their
//! fopdoc specs, and keep the records intact through a save.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 211 IDLM records, 198 of them carrying `IDLF`, `IDLC`, `IDLT` and
//!   `IDLA`. One `IDLA` holds the whole list of animations, and how many it
//!   holds varies: 4, 12 or 16 bytes.
//! - 67 IMGS records, whose `DNAM` is 152 bytes on 44 of them, 148 on twelve
//!   and 132 on eleven — the layout depends on the record's form version.
//! - 31 LGTM records, every one carrying a forty-byte `DATA`.
//! - 208 LSCR records, 23 of them carrying a repeated `LNAM`.
//! - One LSCT record, with an eighty-eight-byte `DATA`.
//! - 12 MICN records, every one carrying an `ICON`.
//! - 20 MUSC records, 19 of them carrying an `FNAM`.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, payloads, saved_subs, sig_order,
    subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// TopsGuardMarker (0x0017179B).
const IDLM_TOPS: u32 = 0x0017_179B;

/// Tops13thFloorImageSpace (0x00175B8E).
const IMGS_TOPS: u32 = 0x0017_5B8E;

/// HiddenValleyInterior (0x00177550).
const LGTM_VALLEY: u32 = 0x0017_7550;

/// NCRCFInterior (0x00176F10).
const LGTM_NCRCF: u32 = 0x0017_6F10;

/// EnclaveLoadScreen01 (0x00166D93).
const LSCR_ENCLAVE: u32 = 0x0016_6D93;

/// StandardLoadingTip (0x00166D58).
const LSCT_TIP: u32 = 0x0016_6D58;

/// MenuIconWildWasteland (0x0017408F).
const MICN_WASTELAND: u32 = 0x0017_408F;

/// EndTransitionStinger (0x00177FA7).
const MUSC_STINGER: u32 = 0x0017_7FA7;

#[test]
fn every_field_of_a_real_idle_animation_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, IDLM_TOPS, *b"IDLM");

    assert_eq!(state.edid, "TopsGuardMarker");

    // --- IDLF: the flag word ---
    assert_eq!(state.idlm_flags, 0, "IDLF: Flags");

    // --- IDLC: the animation count ---
    assert_eq!(state.idlm_animation_count, 3, "IDLC: Animation Count");
    assert_eq!(
        state.idlm_idlc_unused,
        [0, 0, 0],
        "IDLC: the three unused bytes"
    );

    // --- IDLT: the idle timer ---
    assert_eq!(state.idlm_idle_timer, 0.0, "IDLT: Idle Timer Setting");

    // --- IDLA: the animations ---
    assert_eq!(state.idlm_animations.len(), 3, "IDLA: the animations");
    assert_eq!(
        state.idlm_animations[0], 0x000C_8507,
        "IDLA: the first animation"
    );
}

#[test]
fn every_field_of_a_real_image_space_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, IMGS_TOPS, *b"IMGS");

    assert_eq!(state.edid, "Tops13thFloorImageSpace");

    // --- DNAM: the HDR floats ---
    assert_eq!(state.imgs_hdr[0], 0.6, "DNAM: HDR Eye Adapt Speed");
    assert_eq!(state.imgs_hdr[1], 2.0, "DNAM: HDR Blur Radius");
    assert_eq!(state.imgs_hdr[2], 2.0, "DNAM: HDR Blur Passes");
    assert_eq!(state.imgs_hdr[3], 1.0, "DNAM: HDR Emissive Multiplier");

    // --- DNAM: the version-dependent part ---
    assert_eq!(
        state.imgs_rest.len(),
        152 - 56,
        "DNAM: the rest of the struct is carried through"
    );
}

#[test]
fn every_field_of_a_real_lighting_record_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, LGTM_VALLEY, *b"LGTM");

    assert_eq!(state.edid, "HiddenValleyInterior");

    // --- DATA: the three colours ---
    assert_eq!(
        state.lgtm_ambient_color,
        [39, 45, 50, 0],
        "DATA: Ambient Color"
    );
    assert_eq!(
        state.lgtm_directional_color,
        [60, 74, 81, 0],
        "DATA: Directional Color"
    );
    assert_eq!(state.lgtm_fog_color, [23, 27, 32, 0], "DATA: Fog Color");

    // --- DATA: the floats and the rotations ---
    assert_eq!(state.lgtm_fog_near, 32.0, "DATA: Fog Near");
    assert_eq!(state.lgtm_fog_far, 2200.0, "DATA: Fog Far");
    assert_eq!(
        state.lgtm_directional_rotation_xy, 120,
        "DATA: Directional Rotation XY"
    );
    assert_eq!(
        state.lgtm_directional_rotation_z, 270,
        "DATA: Directional Rotation Z"
    );
    assert_eq!(state.lgtm_directional_fade, 1.0, "DATA: Directional Fade");
}

#[test]
fn the_second_lighting_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, LGTM_VALLEY, *b"LGTM");
    assert_eq!(state.lgtm_fog_far, 2200.0);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, LGTM_NCRCF, *b"LGTM");
    assert_eq!(state.edid, "NCRCFInterior");
    assert_eq!(
        state.lgtm_fog_far, 3600.0,
        "DATA: this record's own fog far"
    );
    assert_eq!(
        state.lgtm_ambient_color,
        [24, 35, 41, 0],
        "DATA: this record's own ambient colour"
    );
}

#[test]
fn every_field_of_a_real_load_screen_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, LSCR_ENCLAVE, *b"LSCR");

    assert_eq!(state.edid, "EnclaveLoadScreen01");
    assert_eq!(
        state.lscr_icon, "interface\\loading\\loading_bulletinboard02.dds",
        "ICON: the large icon"
    );
    assert!(
        state
            .lscr_desc
            .starts_with("Though they were a powerful force"),
        "DESC: the description"
    );
    assert_eq!(
        state.lscr_load_screen_type,
        Some(0x0016_6D58),
        "WMI1: Load Screen Type"
    );
}

#[test]
fn every_field_of_a_real_load_screen_type_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, LSCT_TIP, *b"LSCT");

    assert_eq!(state.edid, "StandardLoadingTip");

    // --- DATA: the type and the rectangle ---
    assert_eq!(state.lsct_type, 3, "DATA: Type (Tip)");
    assert_eq!(state.lsct_x, 210, "DATA: X");
    assert_eq!(state.lsct_y, 100, "DATA: Y");
    assert_eq!(state.lsct_width, 850, "DATA: Width");
    assert_eq!(state.lsct_height, 400, "DATA: Height");
    assert_eq!(state.lsct_orientation, 0.0, "DATA: Orientation");

    // --- DATA: the first font ---
    assert_eq!(state.lsct_font1, 6, "DATA: Font 1");
    assert_eq!(state.lsct_font1_color[0], 255.0, "DATA: Font 1 red");
    assert_eq!(state.lsct_font1_color[1], 180.0, "DATA: Font 1 green");
    assert_eq!(state.lsct_font1_color[2], 53.0, "DATA: Font 1 blue");
    assert_eq!(
        state.lsct_font1_alignment, 2,
        "DATA: Font 1 alignment (Center)"
    );

    // --- DATA: the second font and the stats ---
    assert_eq!(state.lsct_font2, 6, "DATA: Font 2");
    assert_eq!(
        state.lsct_font2_alignment, 4,
        "DATA: Font 2 alignment (Right)"
    );
    assert_eq!(state.lsct_stats, 3, "DATA: Stats");
}

#[test]
fn every_field_of_a_real_mission_icon_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, MICN_WASTELAND, *b"MICN");

    assert_eq!(state.edid, "MenuIconWildWasteland");
    assert_eq!(
        state.micn_icon, "interface\\icons\\message icons\\glow_message_vaultboy_wasteland.dds",
        "ICON: the large icon"
    );
}

#[test]
fn every_field_of_a_real_music_record_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, MUSC_STINGER, *b"MUSC");

    assert_eq!(state.edid, "EndTransitionStinger");
    assert_eq!(
        state.musc_filename, "special\\mus_endgametransitionstinger.mp3",
        "FNAM: the filename"
    );
    assert_eq!(state.musc_db, -0.0001, "ANAM: db");
}

#[test]
fn a_load_screen_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = LSCR_ENCLAVE;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"LSCR");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "lscr_identity");

    assert_eq!(after.len(), before.len(), "the same subrecords come back");
    for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
        assert_eq!(
            (b.sig, &b.data),
            (a.sig, &a.data),
            "subrecord {i} ({}) is byte-identical",
            fnv_util::tag_str(&a.sig)
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
    let fid = LSCR_ENCLAVE;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"LSCR");

    // ICON and DESC.
    state.lscr_icon = "textures\\load.dds".to_string();
    state.lscr_desc = "Edited description".to_string();

    // WMI1: the load screen type.
    state.lscr_load_screen_type = Some(0x0012_3456);

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "lscr_edited");

    let icon = payload(&after, b"ICON");
    assert_eq!(
        String::from_utf8_lossy(&icon).trim_end_matches('\0'),
        "textures\\load.dds",
        "the edited icon"
    );
    let desc = payload(&after, b"DESC");
    assert_eq!(
        String::from_utf8_lossy(&desc).trim_end_matches('\0'),
        "Edited description",
        "the edited description"
    );
    let wmi1 = payload(&after, b"WMI1");
    assert_eq!(
        u32::from_le_bytes([wmi1[0], wmi1[1], wmi1[2], wmi1[3]]),
        0x0012_3456,
        "the edited load screen type"
    );
}

#[test]
fn the_image_space_keeps_its_version_dependent_length() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The IMGS DNAM is 152 bytes on 44 of the records, 148 on twelve and 132
    // on eleven. A fixed write would grow the short ones and give them bytes
    // their form version does not have.
    let mut before: std::collections::BTreeMap<u32, usize> = Default::default();
    for fid in common::fids_of(&s, b"IMGS") {
        before.insert(fid, payload(&subs(&s, fid), b"DNAM").len());
        let mut state = ObjectEditorState::default();
        state.load_from_session(&s, fid, *b"IMGS");
        state.save_to_session(&mut s);
    }

    let path = std::env::temp_dir().join("nvre_imgs_len.esp");
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "all.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&path);

    let mut checked = 0;
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"IMGS" {
            return;
        }
        let subs = r.subrecords().expect("subs");
        let len = subs
            .iter()
            .find(|s| s.sig == *b"DNAM")
            .map(|s| s.data.len())
            .expect("a DNAM");
        assert_eq!(
            len, before[&r.header.form_id],
            "{:08X}: DNAM keeps its own length",
            r.header.form_id
        );
        checked += 1;
    });
    assert_eq!(checked, 67, "every IMGS record came back");
}

#[test]
fn the_load_screen_locations_survive_a_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The LNAM locations are a *repeated* subrecord. Written with a
    // per-subrecord edit they would collapse onto the last one.
    let mut before: std::collections::BTreeMap<u32, Vec<Vec<u8>>> = Default::default();
    for fid in common::fids_of(&s, b"LSCR") {
        let lnam = payloads(&subs(&s, fid), b"LNAM");
        if !lnam.is_empty() {
            before.insert(fid, lnam);
        }
    }
    assert!(!before.is_empty(), "some load screens carry locations");

    for fid in common::fids_of(&s, b"LSCR") {
        let mut state = ObjectEditorState::default();
        state.load_from_session(&s, fid, *b"LSCR");
        state.save_to_session(&mut s);
    }

    let path = std::env::temp_dir().join("nvre_lscr_lnam.esp");
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "all.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&path);

    let mut checked = 0;
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
        if r.header.sig != *b"LSCR" {
            return;
        }
        let Some(want) = before.get(&r.header.form_id) else {
            return;
        };
        let subs = r.subrecords().expect("subs");
        let got: Vec<Vec<u8>> = subs
            .iter()
            .filter(|s| s.sig == *b"LNAM")
            .map(|s| s.data.clone())
            .collect();
        assert_eq!(
            &got, want,
            "{:08X}: the locations come back whole and in order",
            r.header.form_id
        );
        checked += 1;
    });
    assert!(checked > 0, "some load screens with locations were checked");
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, LSCR_ENCLAVE, *b"LSCR");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Load Screen - EnclaveLoadScreen01");
}

#[test]
fn every_idle_animation_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"IDLM", 211);
}

#[test]
fn every_image_space_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"IMGS", 67);
}

#[test]
fn every_lighting_record_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"LGTM", 31);
}

#[test]
fn every_load_screen_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"LSCR", 208);
}

#[test]
fn every_load_screen_type_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"LSCT", 1);
}

#[test]
fn every_mission_icon_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"MICN", 12);
}

#[test]
fn every_music_record_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"MUSC", 20);
}
