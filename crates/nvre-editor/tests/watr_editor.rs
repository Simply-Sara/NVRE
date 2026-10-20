//! The water (WATR) editor.
//!
//! The record's `DNAM` is a 196-byte visual data struct: the water, fog,
//! rain, displacement and noise properties, and three colours. The editor
//! gives every field the spec names its own widget, and the struct is
//! written back over the record's own bytes so the bytes the spec does
//! not name survive a save.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm`.

mod common;

use common::{assert_all_save_back_byte_identical, open_master, payload, saved_subs, subs};
use nvre_editor::object_editor::ObjectEditorState;

/// The clean water record (0x0017B612).
const WATR_CLEAN: u32 = 0x0017_B612;

fn state_for(s: &nvre_editor::session::Session, fid: u32) -> ObjectEditorState {
    let mut state = ObjectEditorState::default();
    state.load_from_session(s, fid, *b"WATR");
    state
}

#[test]
fn every_water_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"WATR", 78);
}

#[test]
fn the_visual_data_is_nineteen_six_bytes() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let dnam = payload(&subs(&s, WATR_CLEAN), b"DNAM");
    assert_eq!(dnam.len(), 196, "the visual data struct is 196 bytes");
}

#[test]
fn every_field_of_a_real_water_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let dnam = payload(&subs(&s, WATR_CLEAN), b"DNAM");
    let state = state_for(&s, WATR_CLEAN);
    let v = &state.watr_visual;

    let f = |at: usize| f32::from_le_bytes([dnam[at], dnam[at + 1], dnam[at + 2], dnam[at + 3]]);

    // The spec's offsets.
    assert_eq!(v.sun_power, f(16), "Water Properties - Sun Power");
    assert_eq!(
        v.reflectivity_amount,
        f(20),
        "Water Properties - Reflectivity Amount"
    );
    assert_eq!(v.fresnel_amount, f(24), "Water Properties - Fresnel Amount");
    assert_eq!(
        v.above_fog_near,
        f(32),
        "Fog Properties - Above Water - Fog Near Plane Distance"
    );
    assert_eq!(
        v.above_fog_far,
        f(36),
        "Fog Properties - Above Water - Fog Far Plane Distance"
    );
    assert_eq!(&v.shallow_color[..], &dnam[40..44], "Shallow Color");
    assert_eq!(&v.deep_color[..], &dnam[44..48], "Deep Color");
    assert_eq!(&v.reflection_color[..], &dnam[48..52], "Reflection Color");
    assert_eq!(v.rain_force, f(56), "Rain Simulator - Force");
    assert_eq!(v.rain_velocity, f(60), "Rain Simulator - Velocity");
    assert_eq!(v.rain_falloff, f(64), "Rain Simulator - Falloff");
    assert_eq!(v.rain_dampener, f(68), "Rain Simulator - Dampener");
    assert_eq!(
        v.displacement_starting_size,
        f(72),
        "Displacement Simulator - Starting Size"
    );
    assert_eq!(
        v.displacement_force,
        f(76),
        "Displacement Simulator - Force"
    );
    assert_eq!(
        v.displacement_velocity,
        f(80),
        "Displacement Simulator - Velocity"
    );
    assert_eq!(
        v.displacement_falloff,
        f(84),
        "Displacement Simulator - Falloff"
    );
    assert_eq!(
        v.displacement_dampener,
        f(88),
        "Displacement Simulator - Dampener"
    );
    assert_eq!(
        v.rain_starting_size,
        f(92),
        "Rain Simulator - Starting Size"
    );
    assert_eq!(
        v.noise_scale,
        f(96),
        "Noise Properties - Normals - Noise Scale"
    );
    assert_eq!(
        v.layer_one_wind_direction,
        f(100),
        "Noise Properties - Noise Layer One - Wind Direction"
    );
    assert_eq!(
        v.layer_two_wind_direction,
        f(104),
        "Noise Properties - Noise Layer Two - Wind Direction"
    );
    assert_eq!(
        v.layer_three_wind_direction,
        f(108),
        "Noise Properties - Noise Layer Three - Wind Direction"
    );
    assert_eq!(
        v.layer_one_wind_speed,
        f(112),
        "Noise Properties - Noise Layer One - Wind Speed"
    );
    assert_eq!(
        v.layer_two_wind_speed,
        f(116),
        "Noise Properties - Noise Layer Two - Wind Speed"
    );
    assert_eq!(
        v.layer_three_wind_speed,
        f(120),
        "Noise Properties - Noise Layer Three - Wind Speed"
    );
    assert_eq!(
        v.depth_falloff_start,
        f(124),
        "Noise Properties - Normals - Depth Falloff Start"
    );
    assert_eq!(
        v.depth_falloff_end,
        f(128),
        "Noise Properties - Normals - Depth Falloff End"
    );
    assert_eq!(
        v.above_fog_amount,
        f(132),
        "Fog Properties - Above Water - Fog Amount"
    );
    assert_eq!(
        v.normals_uv_scale,
        f(136),
        "Noise Properties - Normals - UV Scale"
    );
    assert_eq!(
        v.under_fog_amount,
        f(140),
        "Fog Properties - Under Water - Fog Amount"
    );
    assert_eq!(
        v.under_fog_near,
        f(144),
        "Fog Properties - Under Water - Fog Near Plane Distance"
    );
    assert_eq!(
        v.under_fog_far,
        f(148),
        "Fog Properties - Under Water - Fog Far Plane Distance"
    );
    assert_eq!(
        v.distortion_amount,
        f(152),
        "Water Properties - Distortion Amount"
    );
    assert_eq!(v.shininess, f(156), "Water Properties - Shininess");
    assert_eq!(
        v.reflection_hdr_multiplier,
        f(160),
        "Water Properties - Reflection HDR Multiplier"
    );
    assert_eq!(v.light_radius, f(164), "Water Properties - Light Radius");
    assert_eq!(
        v.light_brightness,
        f(168),
        "Water Properties - Light Brightness"
    );
    assert_eq!(
        v.layer_one_uv_scale,
        f(172),
        "Noise Properties - Noise Layer One - UV Scale"
    );
    assert_eq!(
        v.layer_two_uv_scale,
        f(176),
        "Noise Properties - Noise Layer Two - UV Scale"
    );
    assert_eq!(
        v.layer_three_uv_scale,
        f(180),
        "Noise Properties - Noise Layer Three - UV Scale"
    );
    assert_eq!(
        v.layer_one_amplitude_scale,
        f(184),
        "Noise Properties - Noise Layer One - Amplitude Scale"
    );
    assert_eq!(
        v.layer_two_amplitude_scale,
        f(188),
        "Noise Properties - Noise Layer Two - Amplitude Scale"
    );
    assert_eq!(
        v.layer_three_amplitude_scale,
        f(192),
        "Noise Properties - Noise Layer Three - Amplitude Scale"
    );
}

#[test]
fn the_visual_data_keeps_the_bytes_the_spec_does_not_name() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let before = payload(&subs(&s, WATR_CLEAN), b"DNAM");

    let mut state = state_for(&s, WATR_CLEAN);
    state.watr_visual.sun_power = 3.5;
    state.watr_visual.deep_color = [1, 2, 3, 4];
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, WATR_CLEAN, "watr_edit"), b"DNAM");
    assert_eq!(after.len(), before.len(), "the struct keeps its length");
    assert_eq!(
        f32::from_le_bytes([after[16], after[17], after[18], after[19]]),
        3.5,
        "the sun power is written"
    );
    assert_eq!(&after[44..48], &[1, 2, 3, 4], "the deep colour is written");
    // The sixteen unknown bytes at the front and the four unused bytes
    // after the fresnel amount are the record's own.
    assert_eq!(&after[0..16], &before[0..16], "the unknown header survives");
    assert_eq!(&after[28..32], &before[28..32], "the unused bytes survive");
}

#[test]
fn the_related_waters_are_written() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // A water that carries the related waters struct.
    let fid = common::fids_of(&s, b"WATR")
        .into_iter()
        .find(|f| {
            common::payloads(&subs(&s, *f), b"GNAM")
                .first()
                .is_some_and(|g| g.len() >= 12)
        })
        .expect("a water with related waters");
    let before = payload(&subs(&s, fid), b"GNAM");

    let mut state = state_for(&s, fid);
    state.watr_daytime = Some(0x0017_B612);
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "watr_gnam"), b"GNAM");
    assert_eq!(after.len(), before.len(), "the struct keeps its length");
    assert_eq!(
        u32::from_le_bytes([after[0], after[1], after[2], after[3]]),
        0x0017_B612,
        "the daytime water is written"
    );
}

#[test]
fn the_opacity_and_the_flags_are_written() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = state_for(&s, WATR_CLEAN);
    state.watr_opacity = 77;
    state.watr_causes_damage = true;
    state.save_to_session(&mut s);

    let after = saved_subs(&s, WATR_CLEAN, "watr_flags");
    assert_eq!(payload(&after, b"ANAM"), vec![77], "the opacity is written");
    assert_eq!(
        payload(&after, b"FNAM")[0] & 1,
        1,
        "the damage flag is written"
    );
}
