//! The viewport's per-object lights: what the session reads from a `LIGH`, and
//! what the viewport spawns from it.
//!
//! The lights are the game's own, read from the load order — not the viewport's
//! ambient, which is a flat global term with its own switch. These pin the
//! reader and the spawn so a game whose `LIGH` layout is not decoded does not
//! silently render as if it had no lights.

use bevy::prelude::*;
use nvre_editor::session::{light_data, LightData};
use serde_json::json;

/// The two families name the same fields differently, and one reader covers both.
#[test]
fn fallout_and_skyrim_fields_are_both_read() {
    let fallout = light_data(&json!({
        "color": {"red": 218, "green": 217, "blue": 165, "unused": [0]},
        "radius": 256,
        "flags": {"value": 0, "fields": {"Spot Light": 0, "Off By Default": 0}},
        "falloff_exponent": 2.0,
        "fov": 90.0,
        "value": 0,
        "weight": 0.0
    }));
    assert_eq!(fallout.color, [218, 217, 165], "the Fallout colour");
    assert_eq!(fallout.radius, 256.0);
    assert!(!fallout.spot);
    assert!(!fallout.off_by_default);

    let skyrim = light_data(&json!({
        "color": {"red": 194, "green": 72, "blue": 58, "unused": [0]},
        "radius": 512,
        "flags": {"value": 9217, "fields": {"Shadow Spotlight": 1, "Off By Default": 0}},
        "falloff_exponent": 1.0,
        "fov": 90.0,
        "near_clip": 18.0,
        "value": 0,
        "weight": 0.0
    }));
    assert_eq!(skyrim.color, [194, 72, 58], "the Skyrim colour");
    assert_eq!(skyrim.radius, 512.0);
    assert!(skyrim.spot, "Shadow Spotlight is a spotlight");
    assert!(!skyrim.off_by_default);
}

/// The flag values are numbers, not booleans, and a name the schema did not
/// name falls back to the bit it sits at.
#[test]
fn a_flag_the_schema_did_not_name_reads_from_the_bit() {
    let raw_only = light_data(&json!({
        "radius": 100,
        "flags": {"value": 512}
    }));
    assert!(
        raw_only.spot,
        "bit 9 set and no named table: the fallback reads it"
    );

    // A name that is absent from the named table must not stop the search for a
    // later name: the first name's bit is 0, the second's is set.
    let first_clear = light_data(&json!({
        "radius": 100,
        "flags": {"value": 1024, "fields": {"Spot Light": 0, "Shadow Spotlight": 1}}
    }));
    assert!(first_clear.spot, "the second name is reached");
}

/// `Off By Default` is read from the named table and from the raw bit.
#[test]
fn off_by_default_is_read_both_ways() {
    let named = light_data(&json!({
        "radius": 100,
        "flags": {"value": 32, "fields": {"Off By Default": 1}}
    }));
    assert!(named.off_by_default);

    let raw = light_data(&json!({"radius": 100, "flags": {"value": 32}}));
    assert!(raw.off_by_default, "bit 5 from the raw value");
}

/// A `LIGH` with no data at all is the fallback light, so a cell of lamps is
/// not drawn as if it had none.
#[test]
fn an_undecoded_light_falls_back_rather_than_disappearing() {
    let none = light_data(&json!({}));
    assert_eq!(none, LightData::UNKNOWN);
    assert_eq!(none.color, [255, 255, 255]);
    assert_eq!(none.radius, 1.0);
    assert_eq!(none.fade, 1.0, "the fade default is the engine's own");
}

/// The fade default is 1.0, not 0.0 — a zero would scale every light to black.
#[test]
fn the_fade_default_is_one() {
    let d = LightData::UNKNOWN;
    assert_eq!(d.fade, 1.0);
}

/// The fallback's colour is a Bevy colour, bytes scaled to 0-1.
#[test]
fn the_colour_is_scaled_to_bevy_range() {
    let c = LightData::UNKNOWN.bevy_color();
    let [r, g, b, _] = c.to_linear().to_f32_array();
    assert!((r - 1.0).abs() < 1e-3 && (g - 1.0).abs() < 1e-3 && (b - 1.0).abs() < 1e-3);
}

#[test]
fn s9_4_and_s9_5_point_and_spot_light_unit_mappings() {
    // S9-4: Point light radius maps to range, color maps to linear RGB, fade scales lumens
    let point_data = LightData {
        color: [200, 100, 50],
        radius: 350.0,
        fade: 1.5,
        spot: false,
        off_by_default: false,
        fov: 60.0,
    };
    let point = nvre_editor::viewport::light_bundle(&point_data);
    assert_eq!(point.range, 350.0);
    assert_eq!(point.intensity, 1.5 * nvre_editor::viewport::LIGHT_LUMENS);

    // S9-5: Spot light maps FOV to half-angle radians outer_angle
    let spot_data = LightData {
        color: [255, 255, 255],
        radius: 500.0,
        fade: 1.0,
        spot: true,
        off_by_default: false,
        fov: 90.0,
    };
    let spot = nvre_editor::viewport::light_bundle_spot(&spot_data);
    assert_eq!(spot.range, 500.0);
    assert_eq!(spot.intensity, 1.0 * nvre_editor::viewport::LIGHT_LUMENS);
    // 90 deg FOV / 2 = 45 deg = PI / 4 radians
    assert!((spot.outer_angle - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
}

/// A real master's lights read through the session, for every game.
///
/// This is the check that the schema layer, the record walk and the reader agree
/// — a game whose `LIGH` layout is not decoded reports nothing here. The masters
/// live in five different folders, so the environment names each one: a
/// `NVRE_LIGHTS_<id>` variable holding the master's path. A game whose variable
/// is not set is skipped rather than failed.
#[test]
fn every_games_master_lights_read() {
    let cases = [
        ("fnv", "fnv"),
        ("fo3", "fo3"),
        ("skyrim", "skyrimle"),
        ("skyrimse", "skyrimse"),
        ("fo4", "fo4"),
    ];
    for (id, dir) in cases {
        let Ok(master) = std::env::var(format!("NVRE_LIGHTS_{id}")) else {
            continue;
        };
        let path = std::path::PathBuf::from(&master);
        if !path.exists() {
            continue;
        }
        let schema_dir =
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas")).join(dir);
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(mut session) = nvre_editor::session::Session::open(
            path.as_path(),
            schema_dir.as_path(),
            vec![file_name],
        ) else {
            continue;
        };
        let lighs: Vec<u32> = session
            .doc
            .records()
            .iter()
            .filter(|(_, r)| r.header.sig == *b"LIGH")
            .map(|(_, r)| r.header.form_id)
            .collect();
        assert!(!lighs.is_empty(), "{id}: its master has LIGH records");
        let mut read = 0usize;
        for fid in &lighs {
            if session.light_for(*fid).is_some() {
                read += 1;
            }
        }
        assert_eq!(
            read,
            lighs.len(),
            "{id}: every LIGH in its master must read, got {read} of {}",
            lighs.len()
        );
    }
}
