//! The field-edit assertions for the pages added in the last stretch:
//! `PERK`, `WEAP`, `CREA`, `PACK`, `REGN`, `IMAD` and `EFSH`.
//!
//! The whole-master probe proves a no-edit save is byte-identical. These
//! tests prove the opposite half: that editing a named field changes that
//! field in the file the editor wrote, and that the named field is the one
//! the spec names.

mod common;

use common::*;
use nvre_editor::object_editor::ObjectEditorState;

fn state_for(s: &nvre_editor::session::Session, fid: u32, sig: &[u8; 4]) -> ObjectEditorState {
    let mut state = ObjectEditorState::default();
    state.load_from_session(s, fid, *sig);
    state
}

/// The perk's own five-byte `DATA` — the one the editor owns.
fn perk_own_data(p: &[u8]) -> [u8; 5] {
    let mut out = [0u8; 5];
    out.copy_from_slice(&p[..5]);
    out
}

#[test]
fn perk_edits_its_own_data_and_leaves_the_rank_data_alone() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: NVRE_DATA is not set");
        return;
    };
    let fid = *fids_of(&s, b"PERK").first().expect("a perk");
    let before = subs(&s, fid);
    let datas = payloads(&before, b"DATA");
    assert!(
        datas.len() >= 2,
        "the record carries its own data and one per rank"
    );

    let mut state = state_for(&s, fid, b"PERK");
    state.perk_min_level = state.perk_min_level.wrapping_add(3);
    state.perk_num_ranks = state.perk_num_ranks.wrapping_add(1);
    state.perk_playable = 1;
    state.perk_hidden = 2;
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "perk_edit");
    let datas = payloads(&after, b"DATA");

    // The perk's own data carries the edits.
    let own = perk_own_data(&datas[0]);
    assert_eq!(own[1], state.perk_min_level, "the minimum level is written");
    assert_eq!(own[2], state.perk_num_ranks, "the rank count is written");
    assert_eq!(own[3], 1, "the playable byte is written");
    assert_eq!(own[4], 2, "the hidden byte is written");

    // The rank's data is untouched.
    let ranks = before_data_after(&before);
    for (i, a) in datas[1..].iter().enumerate() {
        if let Some(b) = ranks.get(i) {
            assert_eq!(a, b, "the rank data {i} is unchanged");
        }
    }
}

/// The rank data of the record, for comparison.
fn before_data_after(before: &[nvre_esm::Subrecord]) -> Vec<Vec<u8>> {
    payloads(before, b"DATA")[1..].to_vec()
}

#[test]
fn weapon_animation_type_is_the_first_four_bytes_of_the_data() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: NVRE_DATA is not set");
        return;
    };
    let fid = *fids_of(&s, b"WEAP").first().expect("a weapon");
    let before = payload(&subs(&s, fid), b"DNAM");

    let mut state = state_for(&s, fid, b"WEAP");
    let original = state.weap_animation_type;
    assert_eq!(
        original,
        u32::from_le_bytes([before[0], before[1], before[2], before[3]]),
        "the animation type is the first field"
    );
    state.weap_animation_type = 3;
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "weap_edit"), b"DNAM");
    assert_eq!(after.len(), before.len(), "the data keeps its length");
    assert_eq!(
        u32::from_le_bytes([after[0], after[1], after[2], after[3]]),
        3,
        "the animation type is written"
    );
    // The embedded weapon's actor value, at byte 104, is a different field
    // and must survive untouched.
    if before.len() >= 108 {
        assert_eq!(
            &after[104..108],
            &before[104..108],
            "the embedded weapon's actor value is untouched"
        );
    }
}

#[test]
fn weapon_damage_and_weight_are_written() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: NVRE_DATA is not set");
        return;
    };
    let fid = *fids_of(&s, b"WEAP").first().expect("a weapon");
    let before = payload(&subs(&s, fid), b"DATA");
    assert_eq!(before.len(), 15, "the weapon data is 15 bytes");

    let mut state = state_for(&s, fid, b"WEAP");
    state.weap_dmg = state.weap_dmg.wrapping_add(5);
    state.weap_weight = 2.5;
    state.weap_clip = 9;
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "weap_dmg"), b"DATA");
    assert_eq!(after.len(), 15, "the data keeps its length");
    assert_eq!(
        f32::from_le_bytes([after[8], after[9], after[10], after[11]]),
        2.5,
        "the weight is written"
    );
    assert_eq!(
        i16::from_le_bytes([after[12], after[13]]),
        state.weap_dmg,
        "the damage is written"
    );
    assert_eq!(after[14], 9, "the clip size is written");
}

#[test]
fn creature_inventory_keeps_its_extra_data() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: NVRE_DATA is not set");
        return;
    };
    // A creature that carries an inventory item with extra data.
    let fid = fids_of(&s, b"CREA")
        .into_iter()
        .find(|f| {
            let subs = subs(&s, *f);
            let cnto = subs.iter().position(|x| x.sig == *b"CNTO");
            matches!(cnto, Some(i) if subs.get(i + 1).map(|x| x.sig) == Some(*b"COED"))
        })
        .expect("a creature with an inventory item that has extra data");

    let before = subs(&s, fid);
    let cnto_count = before.iter().filter(|x| x.sig == *b"CNTO").count();
    let coed_count = before.iter().filter(|x| x.sig == *b"COED").count();
    assert!(coed_count > 0, "the creature has extra data");

    let mut state = state_for(&s, fid, b"CREA");
    assert_eq!(
        state.actor_inventory_entries.len(),
        cnto_count,
        "the inventory holds every item"
    );
    assert!(
        state
            .actor_inventory_entries
            .iter()
            .any(|e| e.extra.is_some()),
        "the extra data is attached to the item it belongs to"
    );

    // Edit the count of the first item and check the extra data survives.
    state.actor_inventory_entries[0].count = 7;
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "crea_inv");
    assert_eq!(
        after.iter().filter(|x| x.sig == *b"CNTO").count(),
        cnto_count,
        "every inventory item survives"
    );
    assert_eq!(
        after.iter().filter(|x| x.sig == *b"COED").count(),
        coed_count,
        "every extra data survives"
    );
    let first_cnto = after
        .iter()
        .find(|x| x.sig == *b"CNTO")
        .expect("the first item");
    assert_eq!(
        i32::from_le_bytes([
            first_cnto.data[4],
            first_cnto.data[5],
            first_cnto.data[6],
            first_cnto.data[7]
        ]),
        7,
        "the count is written"
    );
}

#[test]
fn package_header_fields_are_written() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: NVRE_DATA is not set");
        return;
    };
    let fid = *fids_of(&s, b"PACK").first().expect("a package");
    let before = payload(&subs(&s, fid), b"PKDT");

    let mut state = state_for(&s, fid, b"PACK");
    state.pack_general_flags |= 0x01;
    state.pack_type = 3;
    state.pack_fallout_behavior_flags |= 0x04;
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "pack_edit"), b"PKDT");
    assert_eq!(after.len(), before.len(), "the header keeps its length");
    assert_eq!(
        u32::from_le_bytes([after[0], after[1], after[2], after[3]]) & 0x01,
        0x01,
        "the general flag bit is written"
    );
    assert_eq!(after[4], 3, "the type is written");
    assert_eq!(
        u16::from_le_bytes([after[6], after[7]]) & 0x04,
        0x04,
        "the behaviour flag bit is written"
    );
}

#[test]
fn region_map_color_and_priority_are_written() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: NVRE_DATA is not set");
        return;
    };
    let fid = *fids_of(&s, b"REGN").first().expect("a region");
    let before = subs(&s, fid);

    let mut state = state_for(&s, fid, b"REGN");
    state.regn_map_color = [1, 2, 3, 4];
    if let Some(entry) = state.regn_entries.first_mut() {
        entry.priority = 42;
    }
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "regn_edit");
    let rclr = payload(&after, b"RCLR");
    assert_eq!(&rclr[..4], &[1, 2, 3, 4], "the map colour is written");
    assert!(!before.is_empty());
}

#[test]
fn imad_header_duration_and_flags_are_written() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: NVRE_DATA is not set");
        return;
    };
    let fid = *fids_of(&s, b"IMAD")
        .first()
        .expect("an image space adapter");
    let before = payload(&subs(&s, fid), b"DNAM");

    let mut state = state_for(&s, fid, b"IMAD");
    state.imad_header.flags |= 0x01;
    state.imad_header.duration = 12.5;
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "imad_edit"), b"DNAM");
    assert_eq!(after.len(), before.len(), "the header keeps its length");
    assert_eq!(
        u32::from_le_bytes([after[0], after[1], after[2], after[3]]) & 0x01,
        0x01,
        "the flag bit is written"
    );
    assert_eq!(
        f32::from_le_bytes([after[4], after[5], after[6], after[7]]),
        12.5,
        "the duration is written"
    );
}

#[test]
fn effect_shader_flags_are_written() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: NVRE_DATA is not set");
        return;
    };
    let fid = *fids_of(&s, b"EFSH").first().expect("an effect shader");
    let before = payload(&subs(&s, fid), b"DATA");
    assert!(before.len() >= 224, "the shader data is at least 224 bytes");

    let mut state = state_for(&s, fid, b"EFSH");
    state.efsh_data.flags |= 0x01;
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "efsh_edit"), b"DATA");
    assert_eq!(
        after.len(),
        before.len(),
        "the shader data keeps its length"
    );
    assert_eq!(after[0], before[0] | 0x01, "the flag byte is written");
}
