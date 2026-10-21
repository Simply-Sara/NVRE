//! The Grass (GRAS) editor reads and writes every field of every subrecord in
//! fopdoc's GRAS spec, and keeps the record intact through a save.
//!
//! The spec lists `EDID`, `OBND`, the model data collection and a
//! thirty-two-byte `DATA`. Several of the `DATA` fields are marked unused;
//! they are carried through unchanged rather than being zeroed.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 24 GRAS records, every one carrying a thirty-two-byte `DATA`.
//! - `GrassWastelandGreen03` (0x0016ACCC): a density of 25, a maximum slope
//!   of 50, a position range of 32.0, a height range of 0.42, a colour range
//!   of 0.195, a wave period of 50.0, and the flags "Uniform Scaling" and
//!   "Fit To Slope".

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// GrassWastelandGreen03 (0x0016ACCC).
const GRAS_GREEN3: u32 = 0x0016_ACCC;

/// GrassWastelandGreen02 (0x0016ACCB).
const GRAS_GREEN2: u32 = 0x0016_ACCB;

#[test]
fn every_field_of_a_real_grass_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, GRAS_GREEN3, *b"GRAS");

    assert_eq!(state.edid, "GrassWastelandGreen03");

    // --- DATA: the density and the slopes ---
    assert_eq!(state.gras_density, 25, "DATA: Density");
    assert_eq!(state.gras_min_slope, 0, "DATA: Min Slope");
    assert_eq!(state.gras_max_slope, 50, "DATA: Max Slope");

    // --- DATA: the water rules ---
    assert_eq!(
        state.gras_unit_from_water_amount, 0,
        "DATA: Unit From Water Amount"
    );
    assert_eq!(
        state.gras_unit_from_water_type, 0,
        "DATA: Unit From Water Type (Above - At Least)"
    );

    // --- DATA: the four floats ---
    assert_eq!(state.gras_position_range, 32.0, "DATA: Position Range");
    assert_eq!(state.gras_height_range, 0.42, "DATA: Height Range");
    assert_eq!(state.gras_color_range, 0.195, "DATA: Color Range");
    assert_eq!(state.gras_wave_period, 50.0, "DATA: Wave Period");

    // --- DATA: the flags ---
    assert_eq!(state.gras_flags, 6, "DATA: Flags");
    assert_eq!(state.gras_flags & 0x01, 0, "0x01 Vertex Lighting is clear");
    assert_ne!(state.gras_flags & 0x02, 0, "0x02 Uniform Scaling");
    assert_ne!(state.gras_flags & 0x04, 0, "0x04 Fit To Slope");

    // --- The model ---
    assert_eq!(
        state.gras_model.modl, "landscape\\grass\\NVGreenGrass03.NIF",
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
    state.load_from_session(&s, GRAS_GREEN3, *b"GRAS");
    assert_eq!(state.gras_density, 25);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, GRAS_GREEN2, *b"GRAS");
    assert_eq!(state.edid, "GrassWastelandGreen02");
    assert_eq!(
        state.gras_model.modl, "landscape\\grass\\NVGreenGrass02.NIF",
        "MODL: this record's own model path"
    );
}

#[test]
fn a_grass_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = GRAS_GREEN3;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"GRAS");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "gras_identity");

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
    let fid = GRAS_GREEN3;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"GRAS");

    // DATA: the density, the slopes, the water rules, the floats and the
    // flags.
    state.gras_density = 77;
    state.gras_min_slope = 5;
    state.gras_max_slope = 60;
    state.gras_unit_from_water_amount = 300;
    state.gras_unit_from_water_type = 4;
    state.gras_position_range = 64.0;
    state.gras_height_range = 1.5;
    state.gras_color_range = 0.75;
    state.gras_wave_period = 25.0;
    state.gras_flags |= 0x01;

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "gras_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 32, "DATA keeps its thirty-two bytes");
    assert_eq!(data[0], 77, "the edited density");
    assert_eq!(data[1], 5, "the edited minimum slope");
    assert_eq!(data[2], 60, "the edited maximum slope");
    assert_eq!(
        u16::from_le_bytes([data[4], data[5]]),
        300,
        "the edited unit from water amount"
    );
    assert_eq!(
        u32::from_le_bytes([data[8], data[9], data[10], data[11]]),
        4,
        "the edited unit from water type"
    );
    let f32_at =
        |at: usize| f32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    assert_eq!(f32_at(12), 64.0, "the edited position range");
    assert_eq!(f32_at(16), 1.5, "the edited height range");
    assert_eq!(f32_at(20), 0.75, "the edited colour range");
    assert_eq!(f32_at(24), 25.0, "the edited wave period");
    assert_ne!(data[28] & 0x01, 0, "the edited Vertex Lighting flag");
    assert_ne!(data[28] & 0x02, 0, "the other flags survive");
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, GRAS_GREEN3, *b"GRAS");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(title, "Grass - GrassWastelandGreen03");
}

#[test]
fn every_grass_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"GRAS", 24);
}
