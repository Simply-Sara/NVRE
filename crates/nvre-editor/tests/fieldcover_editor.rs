//! The field-edit assertions for the types whose pages were completed in
//! the last stretch: the weather, the sound, the weapon, the projectile,
//! the magic effect, the race, the image space adapter and the water.
//!
//! The whole-master probe proves a save with no edit changes nothing.
//! These tests prove the other half: that editing a named field writes
//! that field into the file the editor wrote.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, payloads, saved_subs, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

fn state_for(s: &nvre_editor::session::Session, fid: u32, sig: &[u8; 4]) -> ObjectEditorState {
    let mut state = ObjectEditorState::default();
    state.load_from_session(s, fid, *sig);
    state
}

#[test]
fn the_weapon_animation_data_is_covered() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"WEAP", 261);

    // A weapon with a long animation struct.
    let fid = common::fids_of(&s, b"WEAP")
        .into_iter()
        .find(|f| payload(&subs(&s, *f), b"DNAM").len() >= 200)
        .expect("a weapon with a full animation struct");

    let mut state = state_for(&s, fid, b"WEAP");
    assert!(
        state.weap_anim.len() >= 50,
        "the animation data holds every field the spec names"
    );
    // The animation type is the first field.
    let field = state
        .weap_anim
        .iter_mut()
        .find(|f| f.name == "Animation Type")
        .expect("the animation type");
    let before = match field.value {
        nvre_editor::object_editor::WeapAnimValue::U32(v) => v,
        _ => panic!("the animation type is a uint32"),
    };
    field.value = nvre_editor::object_editor::WeapAnimValue::U32(before + 1);
    // The fire rate is a float32 further in.
    let rate = state
        .weap_anim
        .iter_mut()
        .find(|f| f.name == "Fire Rate")
        .expect("the fire rate");
    rate.value = nvre_editor::object_editor::WeapAnimValue::F32(7.5);
    state.save_to_session(&mut s);

    let dnam = payload(&saved_subs(&s, fid, "weap_anim"), b"DNAM");
    assert_eq!(
        u32::from_le_bytes([dnam[0], dnam[1], dnam[2], dnam[3]]),
        before + 1,
        "the animation type is written"
    );
    assert_eq!(
        f32::from_le_bytes([dnam[64], dnam[65], dnam[66], dnam[67]]),
        7.5,
        "the fire rate is written"
    );
}

#[test]
fn the_weapon_vats_struct_is_covered() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // A weapon that carries the VATS struct.
    let fid = common::fids_of(&s, b"WEAP")
        .into_iter()
        .find(|f| !payloads(&subs(&s, *f), b"VATS").is_empty())
        .expect("a weapon with a VATS struct");

    let mut state = state_for(&s, fid, b"WEAP");
    state.weap_vats_ap = 33.5;
    state.weap_vats_silent = 1;
    state.save_to_session(&mut s);

    let vats = payload(&saved_subs(&s, fid, "weap_vats"), b"VATS");
    assert_eq!(
        f32::from_le_bytes([vats[12], vats[13], vats[14], vats[15]]),
        33.5,
        "the action point cost is written"
    );
    assert_eq!(vats[16], 1, "the silent flag is written");
}

#[test]
fn the_projectile_data_struct_is_covered() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"PROJ", 95);

    let fid = *common::fids_of(&s, b"PROJ").first().expect("a projectile");
    let mut state = state_for(&s, fid, b"PROJ");
    state.proj_tracer_chance = 0.25;
    state.proj_fade_duration = 2.5;
    state.proj_z_rotation = 45.0;
    state.save_to_session(&mut s);

    let data = payload(&saved_subs(&s, fid, "proj_edit"), b"DATA");
    assert_eq!(
        f32::from_le_bytes([data[24], data[25], data[26], data[27]]),
        0.25,
        "the tracer chance is written"
    );
    assert_eq!(
        f32::from_le_bytes([data[48], data[49], data[50], data[51]]),
        2.5,
        "the fade duration is written"
    );
    assert_eq!(
        f32::from_le_bytes([data[76], data[77], data[78], data[79]]),
        45.0,
        "the z rotation is written"
    );
}

#[test]
fn the_magic_effect_data_struct_is_covered() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"MGEF", 289);

    let fid = *common::fids_of(&s, b"MGEF")
        .first()
        .expect("a magic effect");
    let mut state = state_for(&s, fid, b"MGEF");
    state.mgef_projectile_speed = 250.0;
    state.mgef_actor_value = 41;
    state.mgef_archtype = 3;
    state.save_to_session(&mut s);

    let data = payload(&saved_subs(&s, fid, "mgef_edit"), b"DATA");
    assert_eq!(
        f32::from_le_bytes([data[28], data[29], data[30], data[31]]),
        250.0,
        "the projectile speed is written"
    );
    assert_eq!(
        i32::from_le_bytes([data[68], data[69], data[70], data[71]]),
        41,
        "the actor value is written"
    );
    assert_eq!(
        u32::from_le_bytes([data[64], data[65], data[66], data[67]]),
        3,
        "the archetype is written"
    );
}

#[test]
fn the_race_skill_boosts_and_voices_are_covered() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"RACE", 22);

    let fid = *common::fids_of(&s, b"RACE").first().expect("a race");
    let mut state = state_for(&s, fid, b"RACE");
    state.race_skill_boosts[0] = [12, 25];
    state.race_male_hair_color = 7;
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "race_edit");
    let data = payload(&after, b"DATA");
    assert_eq!(data[0] as i8, 12, "the first skill is written");
    assert_eq!(data[1] as i8, 25, "the first boost is written");
    assert_eq!(
        payload(&after, b"CNAM")[0],
        7,
        "the male default hair colour is written"
    );
}

#[test]
fn the_image_space_adapter_tint_colours_are_covered() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"IMGS", 67);

    let fid = *common::fids_of(&s, b"IMGS")
        .first()
        .expect("an image space");
    let mut state = state_for(&s, fid, b"IMGS");
    state.imgs_night_eye_tint = [10, 20, 30];
    state.imgs_cinematic_brightness_tint = [40, 50, 60];
    state.save_to_session(&mut s);

    let dnam = payload(&saved_subs(&s, fid, "imgs_edit"), b"DNAM");
    assert_eq!(
        &dnam[84..87],
        &[10, 20, 30],
        "the night eye tint is written"
    );
    assert_eq!(
        &dnam[104..107],
        &[40, 50, 60],
        "the cinematic brightness tint is written"
    );
}

#[test]
fn the_weather_colours_are_covered() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"WTHR", 63);

    let fid = *common::fids_of(&s, b"WTHR").first().expect("a weather");
    let mut state = state_for(&s, fid, b"WTHR");
    assert!(
        !state.wthr_colors.is_empty(),
        "the weather's colours are read"
    );
    state.wthr_colors[0].sunrise = [1, 2, 3, 4];
    state.wthr_fog_distances[0] = 1234.5;
    state.wthr_cloud_speed_lower = 42;
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "wthr_edit");
    let nam0 = payload(&after, b"NAM0");
    assert_eq!(
        &nam0[0..4],
        &[1, 2, 3, 4],
        "the sky's sunrise colour is written"
    );
    let fnam = payload(&after, b"FNAM");
    assert_eq!(
        f32::from_le_bytes([fnam[0], fnam[1], fnam[2], fnam[3]]),
        1234.5,
        "the day fog distance is written"
    );
    assert_eq!(
        payload(&after, b"DATA")[1],
        42,
        "the lower cloud speed is written"
    );
}
