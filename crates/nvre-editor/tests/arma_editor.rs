//! The Armor Addon (ARMA) editor reads and writes every field of every
//! subrecord in fopdoc's ARMA spec, and keeps the record intact through a
//! save.
//!
//! The spec lists `EDID`, `OBND`, `FULL`, `BMDT`, four model data
//! collections, four icons, `ETYP`, `DATA` and `DNAM`. The interesting parts
//! are the four model instances and the variable-length `DNAM`:
//!
//! - `DNAM` is four bytes on 115 of the game's 131 records and twelve on the
//!   rest. A rebuild from the editor's fields would grow the short ones and
//!   give them a `DT` they never had.
//! - The four model collections are the numbered instances of the shared Model
//!   Data page: `MODL`/`MOD2`/`MOD3`/`MOD4` and so on, with `MODB` only in the
//!   first and `MOSD` in the third.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm` with a
//! raw subrecord dump, not against a fixture written from memory:
//!
//! - 131 ARMA records.
//! - `LegateArmorRightH` (0x001649DF): biped flag 0x10 (Right Hand), general
//!   flag 0x10, equipment type 9 (Hand Wear), value 0, max condition 100,
//!   weight 0.0, a twelve-byte `DNAM`, and a `MODL`/`MOD3` pair.

mod common;

use common::{
    assert_all_save_back_byte_identical, open_master, payload, saved_subs, sig_order, subs,
};
use nvre_editor::object_editor::ObjectEditorState;

/// LegateArmorRightH (0x001649DF): a right-hand piece with a twelve-byte
/// `DNAM` and two model instances.
const ARMA_RIGHT_HAND: u32 = 0x0016_49DF;

/// LegateArmorLefth (0x001649DE): the same shape, a left-hand piece.
const ARMA_LEFT_HAND: u32 = 0x0016_49DE;

#[test]
fn every_field_of_a_real_armor_addon_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ARMA_RIGHT_HAND, *b"ARMA");

    assert_eq!(state.edid, "LegateArmorRightH");
    assert_eq!(state.full_name, "LegateArmor Righthand");

    // --- DATA: the value, the condition and the weight ---
    assert_eq!(state.arma_val, 0, "DATA: Value");
    assert_eq!(state.arma_hp, 100, "DATA: Max Condition");
    assert_eq!(state.arma_weight, 0.0, "DATA: Weight");

    // --- DNAM: the armour rating, the flags and the damage threshold ---
    assert_eq!(state.arma_dnam_len, 12, "DNAM: this record's own length");
    assert_eq!(state.arma_dr, 0, "DNAM: AR");
    assert_eq!(state.arma_dnam_flags, 0, "DNAM: Flags");
    assert_eq!(state.arma_dt, 0.0, "DNAM: DT");

    // --- BMDT: the biped flags and the general flags ---
    assert_eq!(
        state.arma_biped_flags & 0x10,
        0x10,
        "BMDT: the Right Hand biped flag"
    );
    assert_eq!(
        state.arma_biped_flags & 0x08,
        0,
        "BMDT: the Left Hand biped flag is clear"
    );
    assert_eq!(state.arma_bmdt_general & 0x10, 0x10, "BMDT: General Flags");

    // --- ETYP: the equipment type ---
    assert_eq!(state.arma_equip_type, 9, "ETYP: Hand Wear");

    // --- The four model instances ---
    assert_eq!(state.arma_models.len(), 4, "four model collections");
    assert_eq!(
        state.arma_models[0].modl, "armor\\LegateArmor\\LegateArmorRH.NIF",
        "the male biped model"
    );
    assert_eq!(
        state.arma_models[2].modl, "armor\\LegateArmor\\LegateArmorRH.NIF",
        "the female biped model (the third instance, whose path is in MOD3)"
    );
    assert!(
        state.arma_models[1].modl.is_empty(),
        "this record has no male world model"
    );
    assert!(
        state.arma_models[3].modl.is_empty(),
        "this record has no female world model"
    );
}

#[test]
fn the_second_record_reads_its_own_values() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ARMA_RIGHT_HAND, *b"ARMA");
    assert_eq!(state.arma_biped_flags & 0x10, 0x10, "Right Hand");

    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ARMA_LEFT_HAND, *b"ARMA");
    assert_eq!(state.edid, "LegateArmorLefth");
    assert_eq!(
        state.arma_biped_flags & 0x08,
        0x08,
        "BMDT: this record's own Left Hand flag"
    );
    assert_eq!(
        state.arma_biped_flags & 0x10,
        0,
        "BMDT: this record's Right Hand flag is clear"
    );
    assert_eq!(
        state.arma_models[0].modl, "armor\\LegateArmor\\LegateArmorLH.NIF",
        "MODL: this record's own model path"
    );
}

#[test]
fn an_armor_addon_with_no_edits_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = ARMA_RIGHT_HAND;
    let before = subs(&s, fid);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ARMA");
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "arma_identity");

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
fn a_four_byte_dnam_is_not_grown_to_twelve() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // 115 of the game's 131 ARMA records carry a four-byte DNAM: the armour
    // rating and the flag word, with no DT. A rebuild from the editor's
    // fields would grow them to twelve bytes and give them a DT they never
    // had.
    //
    // Every record is loaded and saved, and the file is written once at the
    // end: each `Session::save()` rewrites the whole master, so saving once
    // per record would turn a 131-record probe into gigabytes of writes.
    let fids = common::fids_of(&s, b"ARMA");
    let mut before: std::collections::BTreeMap<u32, usize> = Default::default();
    for fid in &fids {
        before.insert(*fid, payload(&subs(&s, *fid), b"DNAM").len());
        let mut state = ObjectEditorState::default();
        state.load_from_session(&s, *fid, *b"ARMA");
        state.save_to_session(&mut s);
    }

    let path = std::env::temp_dir().join("nvre_arma_dnam_len.esp");
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = nvre_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "all.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&path);

    let mut short = 0;
    let mut long = 0;
    let mut checked = 0;
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        if r.header.sig != *b"ARMA" {
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
        if len == 4 {
            short += 1;
        } else {
            long += 1;
        }
        checked += 1;
    });
    assert_eq!(checked, 131, "every ARMA record came back");
    assert_eq!(short, 115, "115 of the records carry a four-byte DNAM");
    assert_eq!(long, 16, "16 of the records carry a twelve-byte DNAM");
}

#[test]
fn edited_fields_in_every_subrecord_survive_the_save() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let fid = ARMA_RIGHT_HAND;
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, fid, *b"ARMA");

    // DATA: the value, the condition and the weight.
    state.arma_val = 250;
    state.arma_hp = 75;
    state.arma_weight = 3.5;

    // DNAM: the armour rating, the flag and the damage threshold.
    state.arma_dr = 42;
    state.arma_dnam_flags |= 0x0001; // Modulates Voice
    state.arma_dt = 12.5;

    // BMDT: the biped flag word and the general flag byte.
    state.arma_biped_flags |= 0x01; // Head
    state.arma_bmdt_general |= 0x40; // Non-Playable

    // ETYP: the equipment type.
    state.arma_equip_type = 8; // Head Wear

    // The icons.
    state.arma_icon = "textures\\armor\\male.dds".to_string();
    state.arma_mico = "textures\\armor\\male_m.dds".to_string();
    state.arma_ico2 = "textures\\armor\\female.dds".to_string();
    state.arma_mic2 = "textures\\armor\\female_m.dds".to_string();

    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "arma_edited");

    let data = payload(&after, b"DATA");
    assert_eq!(data.len(), 12, "DATA is twelve bytes");
    assert_eq!(
        i32::from_le_bytes([data[0], data[1], data[2], data[3]]),
        250,
        "the edited value"
    );
    assert_eq!(
        i32::from_le_bytes([data[4], data[5], data[6], data[7]]),
        75,
        "the edited max condition"
    );
    assert_eq!(
        f32::from_le_bytes([data[8], data[9], data[10], data[11]]),
        3.5,
        "the edited weight"
    );

    let dnam = payload(&after, b"DNAM");
    assert_eq!(dnam.len(), 12, "DNAM keeps its twelve bytes");
    assert_eq!(
        i16::from_le_bytes([dnam[0], dnam[1]]),
        42,
        "the edited armour rating"
    );
    assert_eq!(
        u16::from_le_bytes([dnam[2], dnam[3]]),
        1,
        "the edited Modulates Voice flag"
    );
    assert_eq!(
        f32::from_le_bytes([dnam[4], dnam[5], dnam[6], dnam[7]]),
        12.5,
        "the edited damage threshold"
    );

    let bmdt = payload(&after, b"BMDT");
    assert_eq!(bmdt.len(), 8, "BMDT is eight bytes");
    assert_eq!(
        u32::from_le_bytes([bmdt[0], bmdt[1], bmdt[2], bmdt[3]]) & 0x01,
        0x01,
        "the edited Head biped flag"
    );
    assert_eq!(bmdt[4] & 0x40, 0x40, "the edited Non-Playable general flag");
    assert_eq!(
        bmdt[4] & 0x10,
        0x10,
        "the general flag byte's unnamed bit survives"
    );

    let etyp = payload(&after, b"ETYP");
    assert_eq!(
        i32::from_le_bytes([etyp[0], etyp[1], etyp[2], etyp[3]]),
        8,
        "the edited equipment type"
    );

    for (sig, want) in [
        (b"ICON", "textures\\armor\\male.dds"),
        (b"MICO", "textures\\armor\\male_m.dds"),
        (b"ICO2", "textures\\armor\\female.dds"),
        (b"MIC2", "textures\\armor\\female_m.dds"),
    ] {
        let got = payload(&after, sig);
        assert_eq!(
            String::from_utf8_lossy(&got).trim_end_matches('\0'),
            want,
            "{}: the edited icon",
            nvre_util::tag_str(sig)
        );
    }
}

#[test]
fn the_window_title_names_the_record_type() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = ObjectEditorState::default();
    state.load_from_session(&s, ARMA_RIGHT_HAND, *b"ARMA");
    let title = nvre_editor::object_editor::window_title(&state.sig, &state.edid, &state.full_name);
    assert_eq!(
        title,
        "Armor Addon - LegateArmorRightH (LegateArmor Righthand)"
    );
}

#[test]
fn every_armor_addon_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"ARMA", 131);
}
