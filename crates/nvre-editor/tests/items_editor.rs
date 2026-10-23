//! The Key (KEYM), Ingredient (INGR), Impact (IPCT), Impact Data Set (IPDS),
//! Recipe (RCPE) and Script (SCPT) editors read and write every field of
//! every subrecord in their fopdoc specs, and keep the records intact
//! through a save.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 313 KEYM records, every one carrying an eight-byte `DATA`.
//! - 1 INGR record, the only one in the game, carrying an eight-byte `ENIT`.
//! - 726 IPCT records, every one carrying a twenty-four-byte `DATA`.
//! - 120 IPDS records, every one carrying a forty-eight-byte `DATA`.
//! - 105 RCPE records, with the ingredients in `RCIL` and the outputs in
//!   `RCOD`, each followed by an `RCQY`.
//! - 2576 SCPT records, with 7837 local variables as `SLSD`/`SCVR` pairs.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, payloads, saved_subs, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// HVCollarKey (0x00175A56).
const KEYM_COLLAR: u32 = 0x0017_5A56;

/// DoNotCreateNewIngredientsWeArentUsingThemInFallout (0x0003135B).
const INGR_ONLY: u32 = 0x0003_135B;

/// RipperImpactOrganic (0x00177874).
const IPCT_RIPPER: u32 = 0x0017_7874;

/// ThrownImpactDataSet (0x00176A12).
const IPDS_THROWN: u32 = 0x0017_6A12;

/// Recipe127mm (0x0017BA3E).
const RCPE_127MM: u32 = 0x0017_BA3E;

/// VMS18CompanionFreezerExitSCRIPT (0x0017B78C).
const SCPT_FREEZER: u32 = 0x0017_B78C;

#[test]
fn every_field_of_a_real_key_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, KEYM_COLLAR, *b"KEYM");

    assert_eq!(state.edid, "HVCollarKey");
    assert_eq!(state.full_name, "Slave Collar Key");

    // --- DATA: the value and the weight ---
    assert_eq!(state.keym_value, 0, "DATA: Value");
    assert_eq!(state.keym_weight, 0.0, "DATA: Weight");

    // --- ICON and MICO ---
    assert_eq!(
        state.keym_icon, "interface\\icons\\pipboyimages\\items\\item_key.dds",
        "ICON: the large icon"
    );
    assert!(
        state.keym_mico.contains("glow_items_key.dds"),
        "MICO: the small icon"
    );

    // --- SCRI ---
    assert_eq!(state.keym_script, Some(0x0017_5A53), "SCRI: Script");
}

#[test]
fn every_field_of_a_real_ingredient_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, INGR_ONLY, *b"INGR");

    assert_eq!(
        state.edid,
        "DoNotCreateNewIngredientsWeArentUsingThemInFallout"
    );

    // --- ETYP ---
    assert_eq!(state.ingr_equipment_type, 0, "ETYP: Equipment Type");

    // --- DATA ---
    assert_eq!(state.ingr_weight, 0.0, "DATA: Weight");

    // --- ENIT ---
    assert_eq!(state.ingr_effect_value, 0, "ENIT: Value");
    assert_eq!(state.ingr_effect_flags, 0, "ENIT: Flags");

    // --- EFID and EFIT ---
    assert_eq!(state.ingr_effects.len(), 1, "the one effect");
    assert_eq!(state.ingr_effects[0].effect, Some(0x0000_014E), "EFID");
    assert_eq!(
        state.ingr_effects[0].parameters.len(),
        20,
        "EFIT: the parameters"
    );
}

#[test]
fn every_field_of_a_real_impact_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, IPCT_RIPPER, *b"IPCT");

    assert_eq!(state.edid, "RipperImpactOrganic");

    // --- DATA ---
    assert_eq!(state.ipct_duration, 0.06, "DATA: Effect - Duration");
    assert_eq!(state.ipct_orientation, 1, "DATA: Effect - Orientation");
    assert_eq!(state.ipct_angle_threshold, 15.0, "DATA: Angle Threshold");
    assert_eq!(state.ipct_placement_radius, 16.0, "DATA: Placement Radius");
    assert_eq!(state.ipct_sound_level, 3, "DATA: Sound Level");
    assert_eq!(state.ipct_flags, 0, "DATA: Flags");

    // --- DNAM ---
    assert_eq!(
        state.ipct_texture_set,
        Some(0x0001_9073),
        "DNAM: Texture Set"
    );

    // --- MODL ---
    assert!(
        state.ipct_model.modl.ends_with("impactRipperOrganic.NIF"),
        "MODL: the model path"
    );

    // --- DODT ---
    let dodt = state.ipct_dodt.as_ref().expect("a DODT");
    assert_eq!(dodt.min_width, 8.0, "DODT: Min Width");
    assert_eq!(dodt.max_width, 24.0, "DODT: Max Width");
    assert_eq!(dodt.min_height, 8.0, "DODT: Min Height");
    assert_eq!(dodt.max_height, 24.0, "DODT: Max Height");
    assert_eq!(dodt.depth, 32.0, "DODT: Depth");
    assert_eq!(dodt.shininess, 4.0, "DODT: Shininess");
    assert_eq!(dodt.parallax_scale, 1.0, "DODT: Parallax Scale");
    assert_eq!(dodt.parallax_passes, 4, "DODT: Parallax Passes");
    assert_eq!(dodt.flags, 0x02, "DODT: Flags");
    assert_eq!(dodt.color, [63, 63, 63, 0], "DODT: Color");
}

#[test]
fn every_field_of_a_real_impact_data_set_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, IPDS_THROWN, *b"IPDS");

    assert_eq!(state.edid, "ThrownImpactDataSet");
    assert_eq!(state.ipds_impacts.len(), 12, "the twelve materials");
    assert_eq!(state.ipds_impacts[0], Some(0x0017_6A10), "DATA: Stone");
    assert_eq!(state.ipds_impacts[3], Some(0x000A_FA75), "DATA: Glass");
    assert_eq!(state.ipds_impacts[6], Some(0x0017_7872), "DATA: Organic");
    assert_eq!(state.ipds_impacts[7], None, "DATA: Cloth is null");
    assert_eq!(state.ipds_impacts[8], None, "DATA: Water is null");
    assert_eq!(
        state.ipds_impacts[11],
        Some(0x0017_7872),
        "DATA: Organic Glow"
    );
}

#[test]
fn every_field_of_a_real_recipe_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, RCPE_127MM, *b"RCPE");

    assert_eq!(state.edid, "Recipe127mm");
    assert_eq!(state.full_name, "12.7mm Round");

    // --- DATA ---
    assert_eq!(state.rcpe_skill, 39, "DATA: Skill (Repair)");
    assert_eq!(state.rcpe_level, 60, "DATA: Level");
    assert_eq!(state.rcpe_category, Some(0x0015_3621), "DATA: Category");
    assert_eq!(
        state.rcpe_sub_category,
        Some(0x0013_E44E),
        "DATA: Sub-Category"
    );

    // --- RCIL and RCQY: the ingredients ---
    assert_eq!(state.rcpe_ingredients.len(), 4, "the ingredients");
    assert_eq!(
        state.rcpe_ingredients[0].item,
        Some(0x0014_0AAA),
        "RCIL: the first ingredient"
    );
    assert_eq!(
        state.rcpe_ingredients[0].quantity, 1,
        "RCQY: the first quantity"
    );
    assert_eq!(
        state.rcpe_ingredients[2].quantity, 12,
        "RCQY: the third quantity"
    );

    // --- RCOD and RCQY: the outputs ---
    assert_eq!(state.rcpe_outputs.len(), 1, "the outputs");
    assert_eq!(
        state.rcpe_outputs[0].item,
        Some(0x0014_29CF),
        "RCOD: the output"
    );
}

#[test]
fn every_field_of_a_real_script_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, SCPT_FREEZER, *b"SCPT");

    assert_eq!(state.edid, "VMS18CompanionFreezerExitSCRIPT");

    // --- SCHR ---
    assert_eq!(state.scpt_header.script_type, 1, "SCHR: Type (Object)");
    assert_eq!(state.scpt_header.version, 0, "SCHR: Version");

    // --- SCDA and SCTX ---
    assert_eq!(state.scpt_compiled.len(), 156, "SCDA: the compiled script");
    assert!(
        state
            .scpt_source
            .starts_with("ScriptName VMS18CompanionFreezerExitSCRIPT"),
        "SCTX: the script source"
    );
}

#[test]
fn a_script_with_local_variables_reads_them() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // Find a script that has local variables.
    let mut found = None;
    for fid in common::fids_of(&s, b"SCPT") {
        if !payloads(&subs(&s, fid), b"SLSD").is_empty() {
            found = Some(fid);
            break;
        }
    }
    let fid = found.expect("a script with local variables");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"SCPT");
    assert!(!state.scpt_variables.is_empty(), "the local variables");
    assert_eq!(
        state.scpt_variables.len(),
        payloads(&subs(&s, fid), b"SLSD").len(),
        "one variable per SLSD"
    );
    // The name comes from the SCVR that follows.
    assert!(
        !state.scpt_variables[0].name.is_empty(),
        "the variable's name"
    );
}

#[test]
fn a_key_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = KEYM_COLLAR;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"KEYM");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "keym_identity");

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
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = IPCT_RIPPER;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"IPCT");

    // DATA.
    state.ipct_duration = 2.5;
    state.ipct_flags = 0x01;
    // DNAM.
    state.ipct_texture_set = Some(0x0012_3456);
    // DODT.
    if let Some(dodt) = state.ipct_dodt.as_mut() {
        dodt.min_width = 3.5;
        dodt.flags = 0x03;
    }

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "ipct_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(
        f32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        2.5,
        "the edited duration"
    );
    assert_eq!(
        u32::from_le_bytes([data[20], data[21], data[22], data[23]]),
        1,
        "the edited flags"
    );
    let dnam = payload(&after, b"DNAM");
    assert_eq!(
        u32::from_le_bytes([dnam[0], dnam[1], dnam[2], dnam[3]]),
        0x0012_3456,
        "the edited texture set"
    );
    let dodt = payload(&after, b"DODT");
    assert_eq!(
        f32::from_le_bytes([dodt[0], dodt[1], dodt[2], dodt[3]]),
        3.5,
        "the edited decal width"
    );
    assert_eq!(dodt[29], 0x03, "the edited decal flags");
}

#[test]
fn the_recipe_keeps_its_ingredients_and_outputs_apart() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The ingredients are RCIL and the outputs are RCOD. A loader that
    // treated them as one list would collapse the two.
    let fid = RCPE_127MM;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"RCPE");
    let ingredients = state.rcpe_ingredients.len();
    let outputs = state.rcpe_outputs.len();
    assert!(outputs > 0, "the recipe has outputs");

    state.rcpe_ingredients[0].quantity = 99;
    state.rcpe_outputs[0].quantity = 77;
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "rcpe_edited");
    let rcil = payloads(&after, b"RCIL");
    let rcod = payloads(&after, b"RCOD");
    let rcqy = payloads(&after, b"RCQY");
    assert_eq!(rcil.len(), ingredients, "the ingredients stay RCIL");
    assert_eq!(rcod.len(), outputs, "the outputs stay RCOD");
    assert_eq!(
        rcqy.len(),
        ingredients + outputs,
        "one quantity for each item"
    );
    assert_eq!(
        u32::from_le_bytes([rcqy[0][0], rcqy[0][1], rcqy[0][2], rcqy[0][3]]),
        99,
        "the edited ingredient's quantity"
    );
    assert_eq!(
        u32::from_le_bytes([
            rcqy[ingredients][0],
            rcqy[ingredients][1],
            rcqy[ingredients][2],
            rcqy[ingredients][3],
        ]),
        77,
        "the edited output's quantity"
    );
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, RCPE_127MM, *b"RCPE");
    let title = nvre_editor::object_editor::window_title(&s.schema, &state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Recipe - Recipe127mm");
}

#[test]
fn every_key_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"KEYM", 313);
}

#[test]
fn every_ingredient_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"INGR", 1);
}

#[test]
fn every_impact_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"IPCT", 726);
}

#[test]
fn every_impact_data_set_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"IPDS", 120);
}

#[test]
fn every_recipe_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"RCPE", 105);
}

#[test]
fn every_script_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"SCPT", 2576);
}
