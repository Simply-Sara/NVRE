//! The Combat Style (CSTY) editor reads and writes every field of every
//! subrecord in fopdoc's CSTY spec, and keeps the record intact through a save.
//!
//! All three subrecords are fixed-size structs — `CSTD` 92 bytes, `CSAD` 84
//! bytes, `CSSD` 64 bytes — and every one of the 84 CSTY records in the game's
//! master carries exactly those sizes. What is checked here is that the values
//! are the game's own values, and that a save reproduces the record byte for
//! byte when nothing was changed.

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
fn subs(s: &Session, fid: u32) -> Vec<fnv_esm::Subrecord> {
    let rec = s.doc.record(fid).expect("the record");
    rec.subrecords().expect("subrecords parse")
}

/// The same, read from the file the editor actually wrote.
fn saved_subs(s: &Session, fid: u32, name: &str) -> Vec<fnv_esm::Subrecord> {
    let path = std::env::temp_dir().join(format!("nvre_csty_{name}.esp"));
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = fnv_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), format!("{name}.esp"))
        .expect("parse");
    let mut found = None;
    fnv_esm::raw::visit_records(&plugin.nodes[1..], &mut |r, _| {
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
fn payload(subs: &[fnv_esm::Subrecord], sig: &[u8; 4]) -> Vec<u8> {
    subs.iter()
        .find(|s| &s.sig == sig)
        .map(|s| s.data.clone())
        .unwrap_or_else(|| panic!("a {} subrecord", nvre_util::tag_str(sig)))
}

fn f32at(b: &[u8], i: usize) -> f32 {
    f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

#[test]
fn every_field_of_a_real_combat_style_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // DefaultCombatstyle: the style every creature inherits from, and the one
    // whose CSTD, CSAD and CSSD all carry non-zero values.
    let fid = 0x0000_003D;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CSTY");

    assert_eq!(state.edid, "DefaultCombatstyle");

    // --- CSTD: Advanced - Standard ---
    let std = &state.csty_std;
    assert_eq!(std.dodge_chance, 75, "Maneuver Decision - Dodge % Chance");
    assert_eq!(
        std.left_right_chance, 50,
        "Maneuver Decision - Left/Right % Chance"
    );
    assert_eq!(std.dodge_lr_timer_min, 0.5, "Dodge L/R Timer Min");
    assert_eq!(std.dodge_lr_timer_max, 1.5, "Dodge L/R Timer Max");
    assert_eq!(std.dodge_forward_timer_min, 0.5, "Dodge Forward Timer Min");
    assert_eq!(std.dodge_forward_timer_max, 1.0, "Dodge Forward Timer Max");
    assert_eq!(std.dodge_back_timer_min, 0.25, "Dodge Back Timer Min");
    assert_eq!(std.dodge_back_timer_max, 0.75, "Dodge Back Timer Max");
    assert_eq!(std.idle_timer_min, 0.5, "Dodge Idle Timer Min");
    assert_eq!(std.idle_timer_max, 1.5, "Dodge Idle Timer Max");
    assert_eq!(std.block_chance, 30, "Melee Decision - Block % Chance");
    assert_eq!(std.attack_chance, 40, "Melee Decision - Attack % Chance");
    assert_eq!(
        std.recoil_stagger_bonus_to_attack, 5.0,
        "Melee Decision - Recoil/Stagger Bonus to Attack"
    );
    assert_eq!(
        std.unconscious_bonus_to_attack, 5.0,
        "Melee Decision - Unconscious Bonus to Attack"
    );
    assert_eq!(
        std.hand_to_hand_bonus_to_attack, 5.0,
        "Melee Decision - Hand-To-Hand Bonus to Attack"
    );
    assert_eq!(
        std.power_attack_chance, 25,
        "Melee Decision - Power Attacks - Power Attack % Chance"
    );
    assert_eq!(
        std.recoil_stagger_bonus_to_power, 5.0,
        "Power Attacks - Recoil/Stagger Bonus to Power"
    );
    assert_eq!(
        std.unconscious_bonus_to_power_attack, 5.0,
        "Power Attacks - Unconscious Bonus to Power Attack"
    );
    assert_eq!(std.power_attack_normal, 20, "Power Attacks - Normal");
    assert_eq!(std.power_attack_forward, 20, "Power Attacks - Forward");
    assert_eq!(std.power_attack_back, 20, "Power Attacks - Back");
    assert_eq!(std.power_attack_left, 20, "Power Attacks - Left");
    assert_eq!(std.power_attack_right, 20, "Power Attacks - Right");
    assert_eq!(std.hold_timer_min, 0.5, "Melee Decision - Hold Timer Min");
    assert_eq!(std.hold_timer_max, 1.5, "Melee Decision - Hold Timer Max");
    // 449 = 0x01C1: Choose Attack Using % Chance, Ignore Damaging Group,
    // Ignore Damaging Spectators and Cannot Use Stealthboy.
    assert_eq!(std.flags, 449, "CSTD Flags");
    assert_ne!(std.flags & 0x0001, 0, "0x0001 Choose Attack Using % Chance");
    assert_ne!(std.flags & 0x0040, 0, "0x0040 Ignore Damaging Group");
    assert_ne!(std.flags & 0x0100, 0, "0x0100 Cannot Use Stealthboy");
    assert_eq!(std.acrobatic_dodge_chance, 0, "Acrobatic Dodge & Chance");
    assert_eq!(std.rushing_attack_chance, 25, "Rushing Attack % Chance");
    assert_eq!(
        std.rushing_attack_distance_mult, 1.0,
        "Rushing Attack Distance Mult"
    );

    // --- CSAD: Advanced - Advanced. None of this was read before. ---
    let adv = &state.csty_adv;
    assert_eq!(adv.dodge_fatigue_mod_mult, -20.0, "Dodge Fatigue Mod Mult");
    assert_eq!(adv.dodge_fatigue_mod_base, 0.0, "Dodge Fatigue Mod Base");
    assert_eq!(
        adv.encumbered_speed_mod_base, -110.0,
        "Encumb. Speed Mod Base"
    );
    assert_eq!(adv.encumbered_speed_mod_mult, 1.0, "Encumb. Speed Mod Mult");
    assert_eq!(
        adv.dodge_while_under_attack_mult, 1.0,
        "Dodge While Under Attack Mult"
    );
    assert_eq!(
        adv.dodge_not_under_attack_mult, 0.75,
        "Dodge Not Under Attack Mult"
    );
    assert_eq!(
        adv.dodge_back_while_under_attack_mult, 1.0,
        "Dodge Back While Under Attack Mult"
    );
    assert_eq!(
        adv.dodge_forward_while_attacking_mult, 1.0,
        "Dodge Forward While Attacking Mult"
    );
    assert_eq!(
        adv.dodge_forward_not_attacking_mult, 0.5,
        "Dodge Forward Not Attacking Mult"
    );
    assert_eq!(
        adv.block_skill_modifier_mult, 20.0,
        "Block Skill Modifier Mult"
    );
    assert_eq!(
        adv.block_while_under_attack_mult, 2.0,
        "Block While Under Attack Mult"
    );
    assert_eq!(
        adv.attack_skill_modifier_mult, 20.0,
        "Attack Skill Modifier Mult"
    );
    assert_eq!(
        adv.attack_while_under_attack_mult, 0.75,
        "Attack While Under Attack Mult"
    );
    assert_eq!(
        adv.attack_during_block_mult, 0.5,
        "Attack During Block Mult"
    );
    assert_eq!(
        adv.power_attack_fatigue_mod_base, 5.0,
        "Power Att. Fatigue Mod Base"
    );
    assert_eq!(
        adv.power_attack_fatigue_mod_mult, -10.0,
        "Power Att. Fatigue Mod Mult"
    );

    // --- CSSD: Simple. Only the first two fields were read before. ---
    let simple = &state.csty_simple;
    assert_eq!(simple.cover_search_radius, 2048.0, "Cover Search Radius");
    assert_eq!(simple.take_cover_chance, 100.0, "Take Cover Chance");
    assert_eq!(simple.wait_timer_min, 2.0, "Wait Timer Min");
    assert_eq!(simple.wait_timer_max, 2.0, "Wait Timer Max");
    assert_eq!(
        simple.wait_to_fire_timer_min, 10.0,
        "Wait to Fire Timer Min"
    );
    assert_eq!(
        simple.wait_to_fire_timer_max, 10.0,
        "Wait to Fire Timer Max"
    );
    assert_eq!(simple.fire_timer_min, 2.0, "Fire Timer Min");
    assert_eq!(simple.fire_timer_max, 2.0, "Fire Timer Max");
    assert_eq!(
        simple.ranged_weapon_range_mult_min, 1.0,
        "Ranged Weapon Range Mult Min"
    );
    assert_eq!(simple.weapon_restrictions, 0, "Weapon Restrictions: None");
    assert_eq!(
        simple.ranged_weapon_range_mult_max, 1.0,
        "Ranged Weapon Range Mult (max)"
    );
    assert_eq!(simple.max_targeting_fov, 0.0, "Max Targeting FOV");
    assert_eq!(simple.combat_radius, 10000.0, "Combat Radius");
    assert_eq!(
        simple.semi_auto_firing_delay_mult_min, 1.0,
        "Semi-Auto Firing Delay Mult Min"
    );
    assert_eq!(
        simple.semi_auto_firing_delay_mult_max, 1.0,
        "Semi-Auto Firing Delay Mult Max"
    );
}

#[test]
fn the_power_attack_chance_is_read_from_the_right_offset() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The old editor read Power Attack % Chance from CSTD offset 24, which is
    // the low byte of the Dodge Back Timer Max float — 0 for every record in
    // the masters. The spec puts it at offset 52, after the three melee bonuses.
    // This is the assertion that catches the offset being wrong again.
    let fid = 0x0000_003D;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CSTY");

    assert_eq!(
        state.csty_std.dodge_back_timer_max, 0.75,
        "offset 24 is inside this float, not the power attack chance"
    );
    assert_eq!(
        state.csty_std.power_attack_chance, 25,
        "Power Attack % Chance is at offset 52"
    );
    // And the byte the old code read really is not the chance.
    let cstd = payload(&subs(&s, fid), b"CSTD");
    assert_eq!(
        cstd[24], 0,
        "offset 24 holds zero, which the old code showed as 0%"
    );
    assert_eq!(cstd[52], 25, "offset 52 holds the real chance");
}

#[test]
fn a_melee_only_style_reports_its_weapon_restriction() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // Find a style that actually restricts weapons, so the enum is exercised
    // against a value other than None.
    let fids: Vec<u32> = s
        .object_groups("")
        .iter()
        .filter(|g| g.sig == *b"CSTY")
        .flat_map(|g| g.objects.iter().map(|(f, _)| *f))
        .collect();
    let mut found = None;
    for fid in fids {
        let mut state = ObjectEditorState::default();
        state.load_from_session(&s, fid, *b"CSTY");
        if state.csty_simple.weapon_restrictions != 0 {
            found = Some((
                fid,
                state.edid.clone(),
                state.csty_simple.weapon_restrictions,
            ));
            break;
        }
    }
    let (fid, edid, restr) = found.expect("a combat style that restricts weapons");
    assert!(
        restr == 1 || restr == 2,
        "Weapon Restrictions is Melee Only (1) or Ranged Only (2), got {restr}"
    );
    let label = if restr == 1 {
        "Melee Only"
    } else {
        "Ranged Only"
    };
    eprintln!("{edid} ({fid:08X}) restricts weapons to {label}");
}

#[test]
fn the_unused_bytes_the_game_fills_are_preserved() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The spec marks CSTD bytes 2..4 "Unused", and xEdit agrees. But 40 of the
    // 84 CSTY records in the masters do not leave them zero: they carry the
    // ASCII fragments "im", "pr", "ial" and ".kf" at offsets 2, 38, 53 and 69 —
    // every one of those 40 carries the same four fragments, and the other 44
    // carry all zeros. That is a leftover string from the editor that wrote
    // them, not a documented field, so the editor must not decide what it
    // means: it copies the bytes through unchanged. Find such a record and
    // check an edit does not disturb them.
    let fids: Vec<u32> = s
        .object_groups("")
        .iter()
        .filter(|g| g.sig == *b"CSTY")
        .flat_map(|g| g.objects.iter().map(|(f, _)| *f))
        .collect();
    let mut target = None;
    for fid in fids {
        let cstd = payload(&subs(&s, fid), b"CSTD");
        if cstd[2..4] != [0, 0] {
            target = Some((fid, cstd[2..4].to_vec()));
            break;
        }
    }
    let Some((fid, bytes)) = target else {
        eprintln!("skipped: no CSTY record fills its Unused bytes");
        return;
    };
    eprintln!("{fid:08X} carries {:?} in CSTD bytes 2..4", bytes);

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CSTY");
    state.csty_std.dodge_chance = 1;
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "unused"), b"CSTD");
    assert_eq!(after[0], 1, "the edited field is written");
    assert_eq!(
        after[2..4],
        bytes,
        "the bytes the spec calls Unused are copied through, not zeroed"
    );
}

#[test]
fn a_combat_style_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = 0x0000_003D;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CSTY");
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
    // And each is the size the spec says it is.
    assert_eq!(payload(&after, b"CSTD").len(), 92, "CSTD is 92 bytes");
    assert_eq!(payload(&after, b"CSAD").len(), 84, "CSAD is 84 bytes");
    assert_eq!(payload(&after, b"CSSD").len(), 64, "CSSD is 64 bytes");
}

#[test]
fn edited_fields_in_all_three_subrecords_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = 0x0000_003D;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CSTY");

    state.csty_std.dodge_chance = 90;
    state.csty_std.block_chance = 60;
    state.csty_std.power_attack_chance = 45;
    state.csty_std.hold_timer_min = 1.25;
    state.csty_std.flags = 0x0003; // the two documented low bits
    state.csty_std.acrobatic_dodge_chance = 15;
    state.csty_std.rushing_attack_chance = 35;
    state.csty_std.rushing_attack_distance_mult = 2.5;

    state.csty_adv.dodge_fatigue_mod_mult = -5.0;
    state.csty_adv.block_while_under_attack_mult = 3.0;
    state.csty_adv.attack_during_block_mult = 0.25;
    state.csty_adv.power_attack_fatigue_mod_mult = -2.0;

    state.csty_simple.cover_search_radius = 512.0;
    state.csty_simple.take_cover_chance = 50.0;
    state.csty_simple.combat_radius = 500.0;
    state.csty_simple.weapon_restrictions = 1; // Melee Only
    state.csty_simple.fire_timer_min = 0.5;
    state.csty_simple.semi_auto_firing_delay_mult_max = 2.0;

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "edited");

    // CSTD, at the spec's own offsets.
    let cstd = payload(&after, b"CSTD");
    assert_eq!(cstd[0], 90, "Dodge % Chance");
    assert_eq!(cstd[36], 60, "Block % Chance");
    assert_eq!(cstd[52], 45, "Power Attack % Chance");
    assert_eq!(f32at(&cstd, 72), 1.25, "Hold Timer Min");
    assert_eq!(
        u16::from_le_bytes([cstd[80], cstd[81]]),
        0x0003,
        "CSTD Flags"
    );
    assert_eq!(cstd[84], 15, "Acrobatic Dodge & Chance");
    assert_eq!(cstd[85], 35, "Rushing Attack % Chance");
    assert_eq!(f32at(&cstd, 88), 2.5, "Rushing Attack Distance Mult");

    // CSAD, as 21 consecutive floats.
    let csad = payload(&after, b"CSAD");
    assert_eq!(f32at(&csad, 0), -5.0, "Dodge Fatigue Mod Mult");
    assert_eq!(f32at(&csad, 48), 3.0, "Block While Under Attack Mult");
    assert_eq!(f32at(&csad, 72), 0.25, "Attack During Block Mult");
    assert_eq!(f32at(&csad, 80), -2.0, "Power Att. Fatigue Mod Mult");

    // CSSD.
    let cssd = payload(&after, b"CSSD");
    assert_eq!(f32at(&cssd, 0), 512.0, "Cover Search Radius");
    assert_eq!(f32at(&cssd, 4), 50.0, "Take Cover Chance");
    assert_eq!(
        u32::from_le_bytes([cssd[40], cssd[41], cssd[42], cssd[43]]),
        1,
        "Weapon Restrictions: Melee Only"
    );
    assert_eq!(f32at(&cssd, 52), 500.0, "Combat Radius");
    assert_eq!(f32at(&cssd, 24), 0.5, "Fire Timer Min");
    assert_eq!(f32at(&cssd, 60), 2.0, "Semi-Auto Firing Delay Mult Max");
}

#[test]
fn an_unnamed_csty_flag_bit_survives_an_edit() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // Find a style with a flag bit the spec does not name set (bit 3 is `??`),
    // then edit a documented bit and check the undocumented one is untouched.
    let fids: Vec<u32> = s
        .object_groups("")
        .iter()
        .filter(|g| g.sig == *b"CSTY")
        .flat_map(|g| g.objects.iter().map(|(f, _)| *f))
        .collect();
    let mut target = None;
    for fid in fids {
        let mut state = ObjectEditorState::default();
        state.load_from_session(&s, fid, *b"CSTY");
        if state.csty_std.flags & 0x0008 != 0 {
            target = Some((fid, state.edid.clone(), state.csty_std.flags));
            break;
        }
    }
    let Some((fid, edid, flags)) = target else {
        eprintln!("skipped: no CSTY record sets the unnamed flag bit 0x0008");
        return;
    };
    eprintln!("{edid} ({fid:08X}) carries flags {flags:#06x}");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"CSTY");
    state.csty_std.flags &= !0x0001; // clear Choose Attack Using % Chance
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "unnamed_flag"), b"CSTD");
    let written = u16::from_le_bytes([after[80], after[81]]);
    assert_eq!(
        written,
        flags & !0x0001,
        "only the documented bit moved; bit 3 is still set"
    );
    assert_ne!(
        written & 0x0008,
        0,
        "the bit the spec marks ?? is preserved"
    );
}
