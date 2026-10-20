//! The field-edit assertions for the types whose pages were completed in
//! the last stretch: the weather, the sound, the weapon, the projectile,
//! the magic effect, the race, the image space adapter and the water.
//!
//! The whole-master probe proves a save with no edit changes nothing.
//! These tests prove the other half: that editing a named field writes
//! that field into the file the editor wrote.
//!
//! Every assertion runs in one test, and the master's 245 MB file is opened
//! once and shared: opening and saving it per assertion turns a suite that
//! should take seconds into one that takes half an hour.

mod common;

use common::{open_master, payload, payloads, saved_subs, subs};
use nvre_editor::object_editor::ObjectEditorState;

fn state_for(s: &nvre_editor::session::Session, fid: u32, sig: &[u8; 4]) -> ObjectEditorState {
    let mut state = ObjectEditorState::default();
    state.load_from_session(s, fid, *sig);
    state
}

#[test]
fn every_named_field_is_written_when_it_is_edited() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };

    // The weapon's animation data.
    {
        // A weapon with the long form of the animation struct.
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

    // The weapon's VATS data.
    {
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

    // The projectile's data.
    {
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

    // The magic effect's data.
    {
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

    // The race's skill boosts and hair colours.
    {
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

    // The spell effect's five named fields.
    {
        let fid = *common::fids_of(&s, b"SPEL").first().expect("a spell");
        let mut state = state_for(&s, fid, b"SPEL");
        assert!(
            !state.spel_effects.is_empty(),
            "the spell has an effect to edit"
        );
        let effect = &mut state.spel_effects[0];
        effect.magnitude = 42;
        effect.area = 7;
        effect.duration = 3;
        effect.effect_type = 1;
        effect.actor_value = -5;
        state.save_to_session(&mut s);

        let efit = payload(&saved_subs(&s, fid, "spel_effect"), b"EFIT");
        assert_eq!(
            u32::from_le_bytes([efit[0], efit[1], efit[2], efit[3]]),
            42,
            "the effect's magnitude is written"
        );
        assert_eq!(
            u32::from_le_bytes([efit[4], efit[5], efit[6], efit[7]]),
            7,
            "the effect's area is written"
        );
        assert_eq!(
            u32::from_le_bytes([efit[8], efit[9], efit[10], efit[11]]),
            3,
            "the effect's duration is written"
        );
        assert_eq!(
            u32::from_le_bytes([efit[12], efit[13], efit[14], efit[15]]),
            1,
            "the effect's type is written"
        );
        assert_eq!(
            i32::from_le_bytes([efit[16], efit[17], efit[18], efit[19]]),
            -5,
            "the effect's actor value is written"
        );
    }

    // The actor data's services, teaching and training level.
    {
        let fid = *common::fids_of(&s, b"NPC_").first().expect("an npc");
        let mut state = state_for(&s, fid, b"NPC_");
        state.actor_services = 0x0000_0003;
        state.actor_teaches = 2;
        state.actor_max_training_level = 5;
        state.actor_aggro_radius_behavior = 1;
        state.save_to_session(&mut s);

        let aidt = payload(&saved_subs(&s, fid, "npc_aidt"), b"AIDT");
        assert_eq!(
            u32::from_le_bytes([aidt[8], aidt[9], aidt[10], aidt[11]]),
            0x0000_0003,
            "the services are written"
        );
        assert_eq!(aidt[12] as i8, 2, "the taught skill is written");
        assert_eq!(aidt[13], 5, "the maximum training level is written");
        assert_eq!(aidt[15], 1, "the aggro radius behaviour is written");
    }

    // The image space adapter's tint colours.
    {
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

    // The weather's colours and fog distances.
    {
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
}
