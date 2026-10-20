//! The sound (SOUN) editor.
//!
//! A sound record carries one of two attenuation structs. The `SNDD` form
//! is 36 bytes and holds everything. The `SNDX` form is 12 bytes and
//! holds only the first ten fields; its curve points, reverb control and
//! priority live in `ANAM`, `GNAM` and `HNAM` instead. The editor reads
//! whichever the record has and writes it back the same way.
//!
//! Every assertion is against a value measured out of `FalloutNV.esm`.

mod common;

use common::{assert_all_save_back_byte_identical, open_master, payload, saved_subs, subs};
use nvre_editor::object_editor::ObjectEditorState;

/// A sound with the long attenuation form (0x0017AE34).
const SOUN_LONG: u32 = 0x0017_AE34;

/// A sound with the short form (0x000C5C22).
const SOUN_SHORT: u32 = 0x000C_5C22;

fn state_for(s: &nvre_editor::session::Session, fid: u32) -> ObjectEditorState {
    let mut state = ObjectEditorState::default();
    state.load_from_session(s, fid, *b"SOUN");
    state
}

#[test]
fn every_sound_in_the_master_saves_back_byte_identical() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_all_save_back_byte_identical(&mut s, b"SOUN", 3190);
}

#[test]
fn the_two_attenuation_forms_are_read() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let long = subs(&s, SOUN_LONG);
    assert_eq!(
        payload(&long, b"SNDD").len(),
        36,
        "the long form is 36 bytes"
    );
    assert!(!long.iter().any(|x| x.sig == *b"SNDX"));

    let short = subs(&s, SOUN_SHORT);
    assert_eq!(
        payload(&short, b"SNDX").len(),
        12,
        "the short form is 12 bytes"
    );
    assert!(!short.iter().any(|x| x.sig == *b"SNDD"));
    assert_eq!(
        payload(&short, b"ANAM").len(),
        10,
        "five attenuation points"
    );
    assert_eq!(payload(&short, b"GNAM").len(), 2, "the reverb control");
    assert_eq!(payload(&short, b"HNAM").len(), 4, "the priority");
}

#[test]
fn every_field_of_a_long_sound_is_loaded() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let d = payload(&subs(&s, SOUN_LONG), b"SNDD");
    let state = state_for(&s, SOUN_LONG);
    let a = &state.sound_attenuation;

    let u8_at = |at: usize| d[at];
    let i16_at = |at: usize| i16::from_le_bytes([d[at], d[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]);
    let i32_at = |at: usize| i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]);

    assert_eq!(a.min_distance, u8_at(0), "Minimum Attenuation Distance");
    assert_eq!(a.max_distance, u8_at(1), "Maximum Attenuation Distancce");
    assert_eq!(
        a.frequency_adjustment, d[2] as i8,
        "Frequency Adjustment Percentage"
    );
    assert_eq!(a.flags, u32_at(4), "Flags");
    assert_eq!(a.static_attenuation, i16_at(8), "Static Attenuation cdB");
    assert_eq!(a.stop_time, u8_at(10), "Stop Time");
    assert_eq!(a.start_time, u8_at(11), "Start Time");
    for (i, point) in a.attenuation_points.iter().enumerate() {
        assert_eq!(*point, i16_at(12 + i * 2), "Attenuation Point {}", i + 1);
    }
    assert_eq!(
        a.reverb_attenuation,
        i16_at(22),
        "Reverb Attenuation Control"
    );
    assert_eq!(a.priority, i32_at(24), "Priority");
    assert_eq!(a.x, i32_at(28), "X");
    // The struct is thirty-two bytes in every one of the game's records,
    // so there is no room for the spec's second coordinate.
    assert_eq!(d.len(), 32, "the attenuation struct is thirty-two bytes");
}

#[test]
fn a_short_sound_keeps_its_points_in_their_own_subrecords() {
    let Some(s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let subs = subs(&s, SOUN_SHORT);
    let anam = payload(&subs, b"ANAM");
    let state = state_for(&s, SOUN_SHORT);
    let a = &state.sound_attenuation;

    for (i, point) in a.attenuation_points.iter().enumerate() {
        assert_eq!(
            *point,
            i16::from_le_bytes([anam[i * 2], anam[i * 2 + 1]]),
            "Attenuation Point {}",
            i + 1
        );
    }
    assert_eq!(
        a.reverb_attenuation,
        {
            let g = payload(&subs, b"GNAM");
            i16::from_le_bytes([g[0], g[1]])
        },
        "Reverb Attenuation Control"
    );
    assert_eq!(
        a.priority,
        {
            let h = payload(&subs, b"HNAM");
            i32::from_le_bytes([h[0], h[1], h[2], h[3]])
        },
        "Priority"
    );
}

#[test]
fn editing_a_long_sound_writes_the_field() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let before = payload(&subs(&s, SOUN_LONG), b"SNDD");

    let mut state = state_for(&s, SOUN_LONG);
    state.sound_attenuation.attenuation_points[2] = 1234;
    state.sound_attenuation.priority = 4321;
    state.sound_attenuation.stop_time = 42;
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, SOUN_LONG, "soun_edit"), b"SNDD");
    assert_eq!(after.len(), before.len(), "the struct keeps its length");
    assert_eq!(
        i16::from_le_bytes([after[17], after[18]]),
        1234,
        "the third attenuation point is written"
    );
    assert_eq!(
        i32::from_le_bytes([after[25], after[26], after[27], after[28]]),
        4321,
        "the priority is written"
    );
    assert_eq!(after[10], 42, "the stop time is written");
    // The unused byte survives.
    assert_eq!(after[3], before[3], "the unused byte survives");
}

#[test]
fn editing_a_short_sound_writes_the_split_subrecords() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let mut state = state_for(&s, SOUN_SHORT);
    state.sound_attenuation.attenuation_points[0] = 999;
    state.sound_attenuation.reverb_attenuation = 77;
    state.sound_attenuation.priority = 88;
    state.save_to_session(&mut s);

    let after = saved_subs(&s, SOUN_SHORT, "soun_short_edit");
    let anam = payload(&after, b"ANAM");
    assert_eq!(
        i16::from_le_bytes([anam[0], anam[1]]),
        999,
        "the first attenuation point is written"
    );
    assert_eq!(
        {
            let g = payload(&after, b"GNAM");
            i16::from_le_bytes([g[0], g[1]])
        },
        77,
        "the reverb control is written"
    );
    assert_eq!(
        {
            let h = payload(&after, b"HNAM");
            i32::from_le_bytes([h[0], h[1], h[2], h[3]])
        },
        88,
        "the priority is written"
    );
    // The short struct keeps its length.
    assert_eq!(
        payload(&after, b"SNDX").len(),
        12,
        "the short form's length"
    );
}
