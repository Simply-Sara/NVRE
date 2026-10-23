//! The Recipe Category (RCCT), Reputation (REPU), Ragdoll (RGDL), Tree
//! (TREE), Texture (TXST), Voice Type (VTYP), Talking Activator (TACT) and
//! Static Collision (SCOL) editors read and write every field of every
//! subrecord in their fopdoc specs, and keep the records intact through a
//! save.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 10 RCCT records, every one carrying a one-byte `DATA` flag word.
//! - 13 REPU records, every one carrying a four-byte `DATA` float.
//! - 38 RGDL records, every one carrying a fourteen-byte `DATA`, a
//!   sixty-byte `RAFD` and a twenty-four-byte `RAPS`.
//! - 3 TREE records, two of them carrying a four-byte `SNAM` and one a
//!   twenty-byte one.
//! - 493 TXST records, 313 of them carrying a two-byte `DNAM` and three a
//!   thirty-six-byte `DODT`.
//! - 100 VTYP records, 91 of them carrying a one-byte `DNAM`.
//! - 87 TACT records, 18 of them carrying a `SCRI` script.
//! - 98 SCOL records, whose `ONAM` and `DATA` interleave: 423 of each.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, payloads, saved_subs, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// ChemsSubRecipes (0x001613CF).
const RCCT_CHEMS: u32 = 0x0016_13CF;

/// RepNVPowderGanger (0x001558E6).
const REPU_GANGER: u32 = 0x0015_58E6;

/// RexRagdoll (0x0017626E).
const RGDL_REX: u32 = 0x0017_626E;

/// AlienRagdoll (0x001690AE).
const RGDL_ALIEN: u32 = 0x0016_90AE;

/// EuonymusBush01 (0x0003D428).
const TREE_EUONYMUS: u32 = 0x0003_D428;

/// NVNCRMPhelmet (0x00177DB0).
const TXST_HELMET: u32 = 0x0017_7DB0;

/// FemaleAdult12 (0x00176579).
const VTYP_FEMALE12: u32 = 0x0017_6579;

/// MaleAdult11 (0x00173561).
const VTYP_MALE11: u32 = 0x0017_3561;

/// TopsRadPackRadio (0x00172FDE).
const TACT_RADPACK: u32 = 0x0017_2FDE;

/// SCOLGoodpringsFenceB01 (0x0017B667).
const SCOL_FENCE: u32 = 0x0017_B667;

#[test]
fn every_field_of_a_real_recipe_category_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, RCCT_CHEMS, *b"RCCT");

    assert_eq!(state.edid, "ChemsSubRecipes");
    assert_eq!(state.full_name, "Chems");

    // --- DATA: the flag word ---
    // The game's own bytes are 0x6B: the Subcategory bit plus bits the spec
    // does not name. The word is kept whole, so those survive.
    assert_eq!(state.rcct_flags, 0x6B, "DATA: Flags");
    assert_eq!(
        state.rcct_flags & 0x01,
        0x01,
        "DATA: the Subcategory bit is set"
    );
    assert_eq!(
        state.rcct_flags & !0x01,
        0x6A,
        "DATA: the unnamed bits are carried through"
    );
}

#[test]
fn every_field_of_a_real_reputation_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, REPU_GANGER, *b"REPU");

    assert_eq!(state.edid, "RepNVPowderGanger");
    assert_eq!(state.full_name, "Powder Gangers");

    // --- DATA: the value ---
    assert_eq!(state.repu_value, 30.0, "DATA: Value");

    // --- ICON: the large icon ---
    assert_eq!(
        state.repu_icon,
        "interface\\icons\\pipboyimages\\reputations\\reputations_powder_ganger.dds",
        "ICON: the large icon"
    );
}

#[test]
fn every_field_of_a_real_ragdoll_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, RGDL_REX, *b"RGDL");

    assert_eq!(state.edid, "RexRagdoll");

    // --- NVER: the version ---
    assert_eq!(state.rgdl_version, 1, "NVER: Version");

    // --- DATA: the bone count and the five flags ---
    assert_eq!(state.rgdl_dynamic_bone_count, 0, "DATA: Dynamic Bone Count");
    assert_eq!(state.rgdl_feedback_enabled, 0, "DATA: Feedback Enabled");
    assert_eq!(state.rgdl_foot_ik_enabled, 0, "DATA: Foot IK Enabled");
    assert_eq!(state.rgdl_look_ik_enabled, 0, "DATA: Look IK Enabled");
    assert_eq!(state.rgdl_grab_ik_enabled, 0, "DATA: Grab IK Enabled");
    assert_eq!(state.rgdl_pose_matching, 0, "DATA: Pose Matching");

    // --- TNAM: the body part data ---
    assert_eq!(
        state.rgdl_body_part_data,
        Some(0x0017_626D),
        "TNAM: Body Part Data"
    );

    // --- RAFD: the feedback gains ---
    assert_eq!(
        state.rgdl_dynamic_keyframe_blend, 0.8,
        "RAFD: Dynamic / Keyframe Blend Amount"
    );
    assert_eq!(state.rgdl_hierarchy_gain, 0.17, "RAFD: Hierarchy Gain");
    assert_eq!(state.rgdl_position_gain, 0.05, "RAFD: Position Gain");
    assert_eq!(state.rgdl_velocity_gain, 0.6, "RAFD: Velocity Gain");
    assert_eq!(state.rgdl_acceleration_gain, 1.0, "RAFD: Acceleration Gain");
    assert_eq!(state.rgdl_snap_gain, 0.1, "RAFD: Snap Gain");
    assert_eq!(state.rgdl_velocity_damping, 0.0, "RAFD: Velocity Damping");
    assert_eq!(
        state.rgdl_snap_max_linear_velocity, 3.0,
        "RAFD: Snap Max Linear Velocity"
    );
    assert_eq!(
        state.rgdl_snap_max_angular_velocity, 0.3,
        "RAFD: Snap Max Angular Velocity"
    );

    // --- RAPS: the match bones and the flags ---
    assert_eq!(state.rgdl_match_bone1, 65535, "RAPS: Match Bone 1");
    assert_eq!(state.rgdl_match_bone2, 65535, "RAPS: Match Bone 2");
    assert_eq!(state.rgdl_match_bone3, 65535, "RAPS: Match Bone 3");
    assert_eq!(state.rgdl_aps_flags, 0, "RAPS: Flags");
    assert_eq!(state.rgdl_motors_strength, 0.0, "RAPS: Motors Strength");
    assert_eq!(
        state.rgdl_match_error_allowance, 0.1,
        "RAPS: Match Error Allowance"
    );
}

#[test]
fn the_second_ragdoll_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, RGDL_ALIEN, *b"RGDL");
    assert_eq!(state.edid, "AlienRagdoll");
    assert_eq!(
        state.rgdl_body_part_data,
        Some(0x0016_90AD),
        "TNAM: this record's own body part data"
    );
}

#[test]
fn every_field_of_a_real_tree_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, TREE_EUONYMUS, *b"TREE");

    assert_eq!(state.edid, "EuonymusBush01");

    // --- SNAM: the seeds ---
    assert_eq!(state.tree_seeds.len(), 1, "SNAM: the SpeedTree seeds");
    assert_eq!(state.tree_seeds[0], 301_409, "SNAM: the seed");

    // --- CNAM: the eight fields ---
    assert_eq!(state.tree_leaf_curvature, 2.5, "CNAM: Leaf Curvature");
    assert_eq!(state.tree_min_leaf_angle, 5.0, "CNAM: Minimum Leaf Angle");
    assert_eq!(state.tree_max_leaf_angle, 85.0, "CNAM: Maximum Leaf Angle");
    assert_eq!(state.tree_branch_dimming, 0.2, "CNAM: Branch Dimming Value");
    assert_eq!(state.tree_leaf_dimming, 0.3, "CNAM: Leaf Dimming Value");
    assert_eq!(state.tree_shadow_radius, 128, "CNAM: Shadow Radius");
    assert_eq!(state.tree_rock_speed, 1.0, "CNAM: Rock Speed");
    assert_eq!(state.tree_rustle_speed, 1.0, "CNAM: Rustle Speed");

    // --- BNAM: the billboard dimensions ---
    assert_eq!(state.tree_billboard_width, 256.0, "BNAM: Width");
    assert_eq!(state.tree_billboard_height, 256.0, "BNAM: Height");

    // --- ICON: the large icon ---
    assert_eq!(
        state.tree_icon, "EuonymusBush01.dds",
        "ICON: the large icon"
    );

    // --- MODL: the model ---
    assert_eq!(
        state.tree_model.modl, "\\EuonymusBush01.spt",
        "MODL: the model path"
    );
}

#[test]
fn every_field_of_a_real_texture_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, TXST_HELMET, *b"TXST");

    assert_eq!(state.edid, "NVNCRMPhelmet");

    // --- TX00 and TX01: the base image and the normal map ---
    assert_eq!(
        state.txst_textures[0], "armor\\headgear\\trooperHelmMP.dds",
        "TX00: the base image"
    );
    assert_eq!(
        state.txst_textures[1], "armor\\headgear\\trooperHelm_n.dds",
        "TX01: the normal map"
    );
    assert_eq!(
        state.txst_textures[2], "",
        "TX02: this record has no environment map mask"
    );

    // --- DNAM: the flags ---
    assert_eq!(state.txst_flags, 0x0000, "DNAM: Flags");
}

#[test]
fn every_field_of_a_real_voice_type_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, VTYP_FEMALE12, *b"VTYP");

    assert_eq!(state.edid, "FemaleAdult12");
    assert_eq!(state.vtyp_flags, 0x02, "DNAM: the Female bit is set");
    assert_eq!(
        state.vtyp_flags & 0x01,
        0,
        "DNAM: Allow Default Dialog is clear"
    );
}

#[test]
fn the_second_voice_type_reads_its_own_flags() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, VTYP_MALE11, *b"VTYP");
    assert_eq!(state.edid, "MaleAdult11");
    assert_eq!(state.vtyp_flags, 0x00, "DNAM: this record's own flags");
}

#[test]
fn every_field_of_a_real_talking_activator_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, TACT_RADPACK, *b"TACT");

    assert_eq!(state.edid, "TopsRadPackRadio");
    assert_eq!(state.full_name, "rad pack");

    // --- VNAM: the voice type ---
    assert_eq!(state.tact_voice_type, Some(0x0013_C8D8), "VNAM: Voice Type");

    // --- INAM: the radio template ---
    assert_eq!(
        state.tact_radio_template,
        Some(0x0017_2D24),
        "INAM: Radio Template"
    );

    // --- MODL: the model ---
    assert_eq!(
        state.tact_model.modl, "clutter\\babycarriage\\babycarriageclean01.nif",
        "MODL: the model path"
    );
}

#[test]
fn every_field_of_a_real_static_collision_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, SCOL_FENCE, *b"SCOL");

    assert_eq!(state.edid, "SCOLGoodpringsFenceB01");

    // The collection interleaves ONAM and DATA, so the parts are a list.
    assert_eq!(state.scol_parts.len(), 4, "the four collision parts");

    // --- the first part: the static and its two placements ---
    assert_eq!(
        state.scol_parts[0].static_ref,
        Some(0x0010_5871),
        "ONAM: the first part's static"
    );
    assert_eq!(
        state.scol_parts[0].placements.len(),
        2,
        "DATA: the first part's placements"
    );
    assert_eq!(
        state.scol_parts[0].placements[0][0], -788.367_2,
        "DATA: the first placement's X"
    );
    assert_eq!(
        state.scol_parts[0].placements[0][6], 1.0,
        "DATA: the first placement's scale"
    );

    // --- the last part ---
    assert_eq!(
        state.scol_parts[3].static_ref,
        Some(0x0010_665B),
        "ONAM: the last part's static"
    );
    assert_eq!(
        state.scol_parts[3].placements.len(),
        1,
        "DATA: the last part's placements"
    );

    // --- MODL: the model ---
    assert_eq!(
        state.scol_model.modl, "SCOL\\SCOLGoodpringsFenceB01.NIF",
        "MODL: the model path"
    );
}

#[test]
fn a_static_collision_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = SCOL_FENCE;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"SCOL");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "scol_identity");

    assert_eq!(after.len(), before.len(), "the same subrecords come back");
    for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
        assert_eq!(
            (b.sig, &b.data),
            (a.sig, &a.data),
            "subrecord {i} ({}) is byte-identical",
            nvre_util::tag_str(&a.sig)
        );
    }
}

#[test]
fn the_collision_parts_keep_their_order_and_their_statics() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The ONAM and DATA subrecords interleave, so a pair of list edits would
    // gather the ONAMs together and separate them from their DATA. They are
    // written as one sequence instead.
    type Parts = Vec<(Vec<u8>, Vec<u8>)>;
    let mut before: std::collections::BTreeMap<u32, Parts> = Default::default();
    for fid in common::fids_of(&s, b"SCOL") {
        let mut parts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        let mut pending: Option<Vec<u8>> = None;
        for sub in subs(&s, fid) {
            if sub.sig == *b"ONAM" {
                if let Some(d) = pending.take() {
                    parts.push((Vec::new(), d));
                }
                pending = Some(sub.data.clone());
            } else if sub.sig == *b"DATA" {
                parts.push((pending.take().unwrap_or_default(), sub.data.clone()));
            }
        }
        before.insert(fid, parts);
    }

    for fid in common::fids_of(&s, b"SCOL") {
        let mut state = ObjectEditorState::default();
        state.load_from_session(&s, fid, *b"SCOL");
        state.save_to_session(&mut s);
    }

    let path = std::env::temp_dir().join("nvre_scol_parts.esp");
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = nvre_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "all.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&path);

    let mut checked = 0;
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        if r.header.sig != *b"SCOL" {
            return;
        }
        let want = &before[&r.header.form_id];
        let subs = r.subrecords().expect("subs");
        let mut got: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        let mut pending: Option<Vec<u8>> = None;
        for sub in subs {
            if sub.sig == *b"ONAM" {
                if let Some(d) = pending.take() {
                    got.push((Vec::new(), d));
                }
                pending = Some(sub.data.clone());
            } else if sub.sig == *b"DATA" {
                got.push((pending.take().unwrap_or_default(), sub.data.clone()));
            }
        }
        assert_eq!(
            &got, want,
            "{:08X}: the collision parts come back whole and in order",
            r.header.form_id
        );
        checked += 1;
    });
    assert_eq!(checked, 98, "every SCOL record came back");
}

#[test]
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = RGDL_REX;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"RGDL");

    // NVER.
    state.rgdl_version = 2;
    // DATA.
    state.rgdl_dynamic_bone_count = 7;
    state.rgdl_feedback_enabled = 1;
    // RAFD.
    state.rgdl_hierarchy_gain = 2.5;
    // RAPS.
    state.rgdl_match_bone1 = 12;
    state.rgdl_aps_flags = 0x01;

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "rgdl_edited");

    let nver = payload(&after, b"NVER");
    assert_eq!(
        u32::from_le_bytes([nver[0], nver[1], nver[2], nver[3]]),
        2,
        "the edited version"
    );
    let data = payload(&after, b"DATA");
    assert_eq!(
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        7,
        "the edited dynamic bone count"
    );
    assert_eq!(data[8], 1, "the edited feedback flag");
    let rafd = payload(&after, b"RAFD");
    assert_eq!(
        f32::from_le_bytes([rafd[4], rafd[5], rafd[6], rafd[7]]),
        2.5,
        "the edited hierarchy gain"
    );
    let raps = payload(&after, b"RAPS");
    assert_eq!(
        u16::from_le_bytes([raps[0], raps[1]]),
        12,
        "the edited match bone"
    );
    assert_eq!(raps[6], 0x01, "the edited pose matching flag");
}

#[test]
fn the_ragdoll_unnamed_bytes_survive_a_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The DATA struct has four unused bytes and one more unused byte, and
    // RAPS has an unused byte. They are not named in the spec, so they are
    // carried through rather than zeroed.
    let fid = RGDL_REX;
    let before_data = payload(&subs(&s, fid), b"DATA");
    let before_raps = payload(&subs(&s, fid), b"RAPS");
    assert_eq!(before_data[4], 0, "DATA: the first unused byte");
    assert_eq!(before_data[13], 0x97, "DATA: the last unused byte");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"RGDL");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "rgdl_unused");

    let data = payload(&after, b"DATA");
    assert_eq!(data[4], before_data[4], "DATA: the first unused byte");
    assert_eq!(data[13], before_data[13], "DATA: the last unused byte");
    let raps = payload(&after, b"RAPS");
    assert_eq!(raps[7], before_raps[7], "RAPS: the unused byte");
}

#[test]
fn the_recipe_category_keeps_its_unnamed_flag_bits() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = RCCT_CHEMS;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"RCCT");

    // Clear the named bit; the unnamed ones must stay.
    state.rcct_flags &= !0x01;
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "rcct_unnamed");
    let data = payload(&after, b"DATA");
    assert_eq!(data[0], 0x6A, "the unnamed bits survive the edit");
}

#[test]
fn the_texture_keeps_its_decal_data() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // Three TXST records carry a DODT. The decal data is written back over
    // the record's own bytes, so the unnamed bytes survive.
    let with_dodt: Vec<u32> = common::fids_of(&s, b"TXST")
        .into_iter()
        .filter(|fid| {
            common::payloads(&subs(&s, *fid), b"DODT")
                .first()
                .is_some_and(|d| d.len() == 36)
        })
        .collect();
    assert_eq!(with_dodt.len(), 3, "three textures carry decal data");

    let fid = with_dodt[0];
    let before = payload(&subs(&s, fid), b"DODT");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"TXST");
    state.txst_dodt_min_width = 1.5;
    state.txst_dodt_flags = 0x03;
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "txst_dodt");
    let dodt = payload(&after, b"DODT");
    assert_eq!(
        f32::from_le_bytes([dodt[0], dodt[1], dodt[2], dodt[3]]),
        1.5,
        "the edited min width"
    );
    assert_eq!(dodt[29], 0x03, "the edited flags");
    assert_eq!(&dodt[30..32], &before[30..32], "the unnamed bytes survive");
    assert_eq!(&dodt[32..36], &before[32..36], "the colour survives");
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, TACT_RADPACK, *b"TACT");
    let title = nvre_editor::object_editor::window_title(&s.schema, &state.sig, &state.edid, &state.full_name);
    // The title carries the record's name as well as its editor id.
    assert_eq!(title, "Talking Activator - TopsRadPackRadio (rad pack)");
}

#[test]
fn every_recipe_category_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"RCCT", 10);
}

#[test]
fn every_reputation_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"REPU", 13);
}

#[test]
fn every_ragdoll_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"RGDL", 38);
}

#[test]
fn every_tree_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"TREE", 3);
}

#[test]
fn every_texture_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"TXST", 493);
}

#[test]
fn every_voice_type_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"VTYP", 100);
}

#[test]
fn every_talking_activator_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"TACT", 87);
}

#[test]
fn every_static_collision_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"SCOL", 98);
}

/// The unused `payloads` import keeps the shared helper available for the
/// repeated-subrecord assertions above.
#[test]
fn the_repeated_payloads_helper_works() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let onams = payloads(&subs(&s, SCOL_FENCE), b"ONAM");
    assert_eq!(onams.len(), 4, "the fence has four statics");
}
