//! What a cell's own lighting reads as, and how the viewport draws it.
//!
//! The cell's `XCLL` is the ambient colour, the sun and the fog the game draws
//! that cell with. These pin the reader — one layout across all five games,
//! since the 40-byte prefix is shared — and the two cases the viewport has to
//! get right: a cell with fog, and a cell with none.

use bevy::prelude::*;
use nvre_editor::session::{cell_lighting, CellLighting};

/// The 40 bytes an interior cell stores: the three colours, then the fog
/// distances and the sun's angles.
///
/// Measured on FalloutNV.esm's `1ECisternA`, whose ambient is (30, 46, 62) and
/// whose fog runs 32 to 1600.
fn cistern_a() -> Vec<u8> {
    let mut b = vec![0u8; 40];
    b[0..3].copy_from_slice(&[30, 46, 62]);
    b[4..7].copy_from_slice(&[136, 137, 75]);
    b[8..11].copy_from_slice(&[29, 34, 50]);
    b[12..16].copy_from_slice(&32.0f32.to_le_bytes());
    b[16..20].copy_from_slice(&1600.0f32.to_le_bytes());
    b[20..24].copy_from_slice(&120i32.to_le_bytes());
    b[24..28].copy_from_slice(&270i32.to_le_bytes());
    b[28..32].copy_from_slice(&1.0f32.to_le_bytes());
    b[32..36].copy_from_slice(&1700.0f32.to_le_bytes());
    b[36..40].copy_from_slice(&0.6f32.to_le_bytes());
    b
}

#[test]
fn a_cells_lighting_reads() {
    let l = cell_lighting(&cistern_a()).expect("40 bytes is enough");
    assert_eq!(l.ambient, [30, 46, 62], "the ambient colour");
    assert_eq!(l.directional, [136, 137, 75], "the sun's colour");
    assert_eq!(l.fog, [29, 34, 50], "the fog colour");
    assert_eq!(l.fog_near, 32.0);
    assert_eq!(l.fog_far, 1600.0);
    assert_eq!(l.rotation_xy, 120);
    assert_eq!(l.rotation_z, 270);
    assert_eq!(l.directional_fade, 1.0);
    assert_eq!(l.fog_clip_dist, 1700.0);
    assert_eq!(l.fog_power, 0.6);
}

/// A subrecord shorter than the layout is not a lighting subrecord at all.
#[test]
fn a_short_subrecord_reads_as_nothing() {
    assert!(cell_lighting(&[]).is_none());
    assert!(cell_lighting(&[0u8; 39]).is_none(), "one byte short");
    assert!(cell_lighting(&cistern_a()).is_some());
}

/// A longer subrecord (>= 92 bytes) decodes directional ambient fields.
#[test]
fn a_longer_subrecord_reads_directional_ambient() {
    let mut long = cistern_a();
    // 52 additional bytes: 6x4B directional colors, 4B specular, 4B scale, 20B remaining
    let mut extra = vec![0u8; 52];
    extra[0..3].copy_from_slice(&[10, 11, 12]); // X+
    extra[4..7].copy_from_slice(&[20, 21, 22]); // X-
    extra[8..11].copy_from_slice(&[30, 31, 32]); // Y+
    extra[12..15].copy_from_slice(&[40, 41, 42]); // Y-
    extra[16..19].copy_from_slice(&[50, 51, 52]); // Z+
    extra[20..23].copy_from_slice(&[60, 61, 62]); // Z-
    extra[24..27].copy_from_slice(&[70, 71, 72]); // specular
    extra[28..32].copy_from_slice(&1.5f32.to_le_bytes()); // scale
    long.extend_from_slice(&extra);

    let l = cell_lighting(&long).expect("92 bytes parses");
    assert_eq!(l.ambient, [30, 46, 62]);
    assert_eq!(
        l.directional_ambient,
        Some([
            [10, 11, 12],
            [20, 21, 22],
            [30, 31, 32],
            [40, 41, 42],
            [50, 51, 52],
            [60, 61, 62]
        ])
    );
    assert_eq!(l.specular, Some([70, 71, 72]));
    assert_eq!(l.directional_scale, Some(1.5));

    // ambient_color averages the base ambient with the 6 directional axes
    let c = l.ambient_color();
    // Base: [30, 46, 62]
    // Axes sum * 1.5:
    // r: (10+20+30+40+50+60)*1.5 = 210*1.5 = 315. Total r = 30 + 315 = 345. 345/7 = 49.28 -> 49
    // g: (11+21+31+41+51+61)*1.5 = 216*1.5 = 324. Total g = 46 + 324 = 370. 370/7 = 52.85 -> 52
    // b: (12+22+32+42+52+62)*1.5 = 222*1.5 = 333. Total b = 62 + 333 = 395. 395/7 = 56.42 -> 56
    let expected = bevy::color::Color::LinearRgba(bevy::color::LinearRgba::new(
        49.0 / 255.0,
        52.0 / 255.0,
        56.0 / 255.0,
        1.0,
    ));
    assert_eq!(c, expected);
}

/// A cell with no fog stores a `fog_far` of zero, and the viewport must draw
/// no fog rather than a fog that starts and ends at the same place.
#[test]
fn a_cell_with_no_fog_says_so() {
    let mut b = cistern_a();
    b[16..20].copy_from_slice(&0.0f32.to_le_bytes());
    let l = cell_lighting(&b).expect("40 bytes is enough");
    assert!(!l.has_fog(), "a fog that ends at zero is no fog");
}

/// A fog whose far is not beyond its near has nothing to blend over.
#[test]
fn a_degenerate_fog_is_no_fog() {
    let mut b = cistern_a();
    b[16..20].copy_from_slice(&32.0f32.to_le_bytes());
    let l = cell_lighting(&b).expect("40 bytes is enough");
    assert!(!l.has_fog(), "equal near and far is no range");
}

/// The colours are bytes scaled to the 0-1 range Bevy wants, and passed as
/// linear RGB directly without double-gamma darkening.
#[test]
fn the_colours_scale_to_bevys_range() {
    let l = cell_lighting(&cistern_a()).expect("40 bytes is enough");
    let [r, g, b, a] = l.ambient_color().to_linear().to_f32_array();
    assert!((r - 30.0 / 255.0).abs() < 1e-4, "red was {r}");
    assert!((g - 46.0 / 255.0).abs() < 1e-4, "green was {g}");
    assert!((b - 62.0 / 255.0).abs() < 1e-4, "blue was {b}");
    assert_eq!(a, 1.0, "the alpha is one");
}

/// Every game's interiors carry one, read through a real master.
///
/// This is the check that the schema, the record walk and the reader agree: a
/// game whose `XCLL` the session cannot reach reports nothing here. The masters
/// live in five different folders, so the environment names each one: a
/// `NVRE_LIGHTS_<id>` variable holding the master's path. A game whose variable
/// is not set is skipped rather than failed.
#[test]
fn every_games_interior_cells_read_their_lighting() {
    use nvre_editor::game::GameProfile;
    use nvre_editor::session::{CellScope, Session};

    let cases = [
        ("NVRE_LIGHTS_fnv", "fnv", "FalloutNV.esm"),
        ("NVRE_LIGHTS_fo3", "fo3", "Fallout3.esm"),
        ("NVRE_LIGHTS_skyrim", "skyrim", "Skyrim.esm"),
        ("NVRE_LIGHTS_skyrimse", "skyrimse", "Skyrim.esm"),
        ("NVRE_LIGHTS_fo4", "fo4", "Fallout4.esm"),
    ];
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas");
    let mut ran = false;
    let data_fallback = std::env::var("NVRE_DATA").ok();
    for (var, id, master) in cases {
        let master_env = std::env::var(var).ok();
        let data = match (&master_env, &data_fallback) {
            (Some(m), _) => m.clone(),
            (None, Some(d)) if id == "fnv" => d.clone(),
            _ => continue,
        };
        let profile = GameProfile::by_id(id).expect("a known game");
        let master_path = std::path::Path::new(&data).join(profile.master);
        let mut session = Session::open(
            &master_path,
            std::path::Path::new(&format!("{dir}/{}", profile.schema_subdir)),
            vec![master.to_string()],
        )
        .expect("the master opens");
        ran = true;

        let cells = session.cells(CellScope::Interiors);
        let with_lighting = cells
            .iter()
            .filter(|c| session.cell_sub(c.form_id, b"XCLL").is_some())
            .count();
        assert_eq!(
            with_lighting,
            cells.len(),
            "{}: every interior cell carries an XCLL",
            profile.name
        );

        // And the reader turns one into real lighting.
        let first = session
            .cell_sub(cells[0].form_id, b"XCLL")
            .expect("an XCLL");
        let l = cell_lighting(&first).expect("the XCLL is long enough");
        println!(
            "{}: {} interior cells, first {:?} fog {}..{}",
            profile.name,
            cells.len(),
            l.ambient,
            l.fog_near,
            l.fog_far
        );
    }
    assert!(ran, "no game's master was named by NVRE_LIGHTS_*");
}

/// An exterior cell carries no `XCLL` at all — its lighting comes from the
/// weather system — so the viewport must not draw a sun or a fog for one.
#[test]
fn an_exterior_cell_has_no_lighting_of_its_own() {
    use nvre_editor::game::GameProfile;
    use nvre_editor::session::{CellScope, Session};

    let cases = [
        ("NVRE_LIGHTS_fnv", "fnv", "FalloutNV.esm"),
        ("NVRE_LIGHTS_fo3", "fo3", "Fallout3.esm"),
        ("NVRE_LIGHTS_skyrim", "skyrim", "Skyrim.esm"),
        ("NVRE_LIGHTS_skyrimse", "skyrimse", "Skyrim.esm"),
        ("NVRE_LIGHTS_fo4", "fo4", "Fallout4.esm"),
    ];
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas");
    let mut ran = false;
    let data_fallback = std::env::var("NVRE_DATA").ok();
    for (var, id, master) in cases {
        let master_env = std::env::var(var).ok();
        let data = match (&master_env, &data_fallback) {
            (Some(m), _) => m.clone(),
            (None, Some(d)) if id == "fnv" => d.clone(),
            _ => continue,
        };
        let profile = GameProfile::by_id(id).expect("a known game");
        let master_path = std::path::Path::new(&data).join(profile.master);
        let mut session = Session::open(
            &master_path,
            std::path::Path::new(&format!("{dir}/{}", profile.schema_subdir)),
            vec![master.to_string()],
        )
        .expect("the master opens");
        ran = true;

        let first = session.worldspaces().first().map(|w| w.form_id);
        let Some(world) = first else { continue };
        let cells = session.cells(CellScope::Worldspace(world));
        assert!(
            !cells.is_empty(),
            "{}: the worldspace has cells",
            profile.name
        );
        let with_lighting = cells
            .iter()
            .filter(|c| session.cell_sub(c.form_id, b"XCLL").is_some())
            .count();
        assert_eq!(
            with_lighting, 0,
            "{}: an exterior cell's lighting comes from the weather",
            profile.name
        );
    }
    assert!(ran, "no game's master was named by NVRE_LIGHTS_*");
}

/// A default is not needed — a cell either has an `XCLL` or it does not — but
/// the struct is `Copy`, which is what the viewport relies on.
#[test]
fn the_struct_is_copy() {
    let l = cell_lighting(&cistern_a()).expect("40 bytes is enough");
    let m: CellLighting = l;
    assert_eq!(l, m);
}

#[test]
fn s9_6_sun_transform_heading_and_elevation() {
    let mut l = cell_lighting(&cistern_a()).expect("valid cell lighting");
    // cistern_a has rotation_xy = 120, rotation_z = 270
    let tr = nvre_editor::viewport::sun_transform(l);
    // At rotation_z = 270 deg (pitch = 3*PI/2), pitch.sin() = -1.0, cos() = 0.0
    // game_dir = [0, 0, -1]. game_to_bevy maps game Z to bevy Y: [0, -1, 0] * 20000.0
    assert!((tr.translation.y - (-20000.0)).abs() < 1.0);
    assert!(tr.translation.x.abs() < 1.0);
    assert!(tr.translation.z.abs() < 1.0);

    // Test overhead sun: rotation_xy = 0, rotation_z = 90 deg (pitch = PI/2)
    l.rotation_xy = 0;
    l.rotation_z = 90;
    let tr_up = nvre_editor::viewport::sun_transform(l);
    assert!((tr_up.translation.y - 20000.0).abs() < 1.0);
    assert!(tr_up.translation.x.abs() < 1.0);
    assert!(tr_up.translation.z.abs() < 1.0);

    // Test East sun: rotation_xy = 0 (heading +X), rotation_z = 0 (elevation 0)
    // game_dir = [1, 0, 0]. game_to_bevy maps game +X to bevy +X: [20000, 0, 0]
    l.rotation_xy = 0;
    l.rotation_z = 0;
    let tr_east = nvre_editor::viewport::sun_transform(l);
    assert!((tr_east.translation.x - 20000.0).abs() < 1.0);
    assert!(tr_east.translation.y.abs() < 1.0);
    assert!(tr_east.translation.z.abs() < 1.0);

    // Test North sun: rotation_xy = 90 (heading +Y), rotation_z = 0 (elevation 0)
    // game_dir = [0, 1, 0]. game_to_bevy maps game +Y to bevy -Z: [0, 0, -20000]
    l.rotation_xy = 90;
    l.rotation_z = 0;
    let tr_north = nvre_editor::viewport::sun_transform(l);
    assert!(tr_north.translation.x.abs() < 1.0);
    assert!(tr_north.translation.y.abs() < 1.0);
    assert!((tr_north.translation.z - (-20000.0)).abs() < 1.0);
}

#[test]
fn s9_7_fog_power_and_distance_bounds() {
    let l = cell_lighting(&cistern_a()).expect("valid cell lighting");
    // cistern_a has fog_near = 32.0, fog_far = 1600.0, fog_power = 0.6
    assert_eq!(l.fog_near, 32.0);
    assert_eq!(l.fog_far, 1600.0);
    assert!((l.fog_power - 0.6).abs() < 1e-5);
    assert!(l.has_fog());
}
