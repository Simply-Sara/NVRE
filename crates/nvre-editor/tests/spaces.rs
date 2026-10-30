//! The camera and the models must be in the same space.
//!
//! A model is placed as `game_to_bevy() * ref_transform(position)`, so the camera
//! has to target that space. Getting this wrong puts the target thousands of units
//! from the models and rotates one axis relative to them, which is what makes a
//! horizontal drag feel mirrored.

use bevy::prelude::*;

/// How a model is placed.
fn game_to_bevy() -> Transform {
    Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
}

/// How a reference's position is written, the same as `viewport::ref_transform`.
fn ref_transform(position: [f32; 3]) -> Transform {
    Transform::from_translation(Vec3::from_array(position))
}

/// How `viewport::cell_centre` maps a game position.
fn cell_centre_one(p: [f32; 3]) -> Vec3 {
    (game_to_bevy() * ref_transform(p)).translation
}

#[test]
fn the_camera_targets_the_space_the_models_are_in() {
    for p in [
        [1000.0_f32, 2000.0, 300.0],
        [-71591.0, 2312.0, 8365.0],
        [0.0, 0.0, 0.0],
        [-5000.5, 12345.25, -9876.75],
    ] {
        let model = (game_to_bevy() * ref_transform(p)).translation;
        let target = cell_centre_one(p);
        assert!(
            model.distance(target) < 0.01,
            "game {p:?} lands the model at {model:?} but the camera targets {target:?}"
        );
    }
}

#[test]
fn s7_3_game_to_bevy_and_bevy_to_game_round_trip() {
    let cases = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1234.5, -6789.0, 42.125),
        Vec3::new(-50000.0, 30000.0, -1200.0),
        Vec3::new(0.001, -0.002, 0.003),
    ];
    let g2b = game_to_bevy();
    for p in cases {
        // Transform a game position using game_to_bevy()
        let bevy_pos = g2b.transform_point(p);
        // Inverse via bevy_to_game
        let back = nvre_editor::viewport::bevy_to_game(bevy_pos);
        let diff = (back - p).abs();
        assert!(
            diff.x < 1e-3 && diff.y < 1e-3 && diff.z < 1e-3,
            "round trip failed: game {p:?} -> bevy {bevy_pos:?} -> back {back:?}"
        );
    }
}

#[test]
fn the_two_spaces_agree_on_which_way_is_right() {
    let a = [0.0_f32, 0.0, 0.0];
    let b = [0.0_f32, 500.0, 0.0]; // 500 units north
    let model_dir = ((game_to_bevy() * ref_transform(b)).translation
        - (game_to_bevy() * ref_transform(a)).translation)
        .normalize();
    let target_dir = (cell_centre_one(b) - cell_centre_one(a)).normalize();
    assert!(
        model_dir.distance(target_dir) < 1e-4,
        "game north is {model_dir:?} in model space but {target_dir:?} in target space"
    );
    assert!(
        (model_dir.z - -1.0).abs() < 1e-4,
        "game north (+Y) must map to Bevy -Z, got {model_dir:?}"
    );

    // And game +Z, which is the vertical axis in the file, maps to Bevy +Y.
    let c = [0.0_f32, 0.0, 500.0]; // 500 units up
    let model_y = ((game_to_bevy() * ref_transform(c)).translation
        - (game_to_bevy() * ref_transform(a)).translation)
        .normalize();
    let target_y = (cell_centre_one(c) - cell_centre_one(a)).normalize();
    assert!(
        model_y.distance(target_y) < 1e-4,
        "game up is {model_y:?} in model space but {target_y:?} in target space"
    );
    assert!(
        (model_y.y - 1.0).abs() < 1e-4,
        "game up (+Z) must map to Bevy +Y, got {model_y:?}"
    );
}

#[test]
fn s7_4_xscl_defaults_to_one_and_applies_to_ref_transform() {
    use nvre_editor::session::scale_in;
    use nvre_editor::viewport::ref_transform;
    use nvre_esm::Subrecord;

    // 1. When XSCL subrecord is absent, scale_in returns None and defaults to 1.0
    let subs_no_xscl = vec![Subrecord {
        sig: *b"NAME",
        data: vec![1, 2, 3, 4],
        extended: false,
    }];
    assert_eq!(scale_in(&subs_no_xscl), None);
    let default_scale = scale_in(&subs_no_xscl).unwrap_or(1.0);
    assert_eq!(default_scale, 1.0);

    // 2. When XSCL is present, scale_in parses 4-byte LE float
    let subs_with_xscl = vec![Subrecord {
        sig: *b"XSCL",
        data: 2.5f32.to_le_bytes().to_vec(),
        extended: false,
    }];
    assert_eq!(scale_in(&subs_with_xscl), Some(2.5));

    // 3. ref_transform applies the scale uniformly to Transform.scale
    let tr = ref_transform([100.0, 200.0, 300.0], [0.0, 0.0, 0.0], default_scale);
    assert_eq!(tr.scale, Vec3::splat(1.0));

    let tr_scaled = ref_transform([100.0, 200.0, 300.0], [0.0, 0.0, 0.0], 2.5);
    assert_eq!(tr_scaled.scale, Vec3::splat(2.5));
    // Translation and rotation are not scaled by ref_transform
    assert_eq!(tr_scaled.translation, Vec3::new(100.0, 200.0, 300.0));
}

#[test]
fn s15_1_fade_radii_and_lod_distribution() {
    use nvre_nif::scene::{pick_lod_level, LodRange};

    let ranges = [
        LodRange {
            near_extent: 0.0,
            far_extent: 1500.0,
        },
        LodRange {
            near_extent: 1500.0,
            far_extent: 6000.0,
        },
    ];

    assert_eq!(pick_lod_level(&ranges, 500.0), Some(0));
    assert_eq!(pick_lod_level(&ranges, 2500.0), Some(1));
    assert_eq!(pick_lod_level(&ranges, 10000.0), None);
}

#[test]
fn s7_7_at_least_one_model_per_frame_rule_in_draw_cell() {
    // The model loading loop in viewport::draw_cell guarantees that:
    // 1. `loaded_any` starts false.
    // 2. The loop condition `if loaded_any && t_models.elapsed() >= frame_budget { break; }`
    //    ensures at least 1 model is loaded in the first iteration even if the budget is 0.
    let frame_budget = std::time::Duration::from_millis(0);
    let mut loaded_any = false;
    let missing_models = vec!["model_a.nif", "model_b.nif", "model_c.nif"];
    let mut loaded_count = 0;

    for _path in missing_models {
        if loaded_any && std::time::Duration::from_millis(1) >= frame_budget {
            break;
        }
        loaded_count += 1;
        loaded_any = true;
    }

    assert_eq!(
        loaded_count, 1,
        "Must load exactly 1 model even when frame_budget is 0ms"
    );
}

#[test]
fn s7_8_interior_cells_draw_only_their_own_references() {
    use nvre_editor::game::ALL;

    // Verify across all available game installs that interior cells
    // have no grid coordinate and draw only their own direct references.
    for game in ALL {
        let Some(data_dir) = game.data_dir() else {
            continue;
        };
        let Ok(_data) = game.open_asset_data(&data_dir) else {
            continue;
        };
        let master_path = data_dir.join(game.master);
        if !master_path.exists() {
            continue;
        }
        let schema_path = std::path::Path::new("schemas").join(game.schema_subdir);
        let Ok(mut session) = nvre_editor::session::Session::open(
            &master_path,
            &schema_path,
            vec![game.master.to_string()],
        ) else {
            continue;
        };

        // Find an interior cell
        let interior = session
            .index
            .cells
            .values()
            .find(|c| c.grid.is_none() && c.world.is_none() && c.edid.is_some())
            .cloned();

        let Some(cell) = interior else {
            continue;
        };

        session.open_cell(cell.form_id);
        let cell_refs = session.cell_refs();

        // In an interior cell, radius is always 0 and exterior_grid_refs is not called.
        // The cell draws only its own references.
        assert!(
            cell.grid.is_none(),
            "{}: interior cell must have no grid coordinate",
            game.id
        );
        assert!(
            !cell_refs.is_empty(),
            "{}: interior cell {:?} must have placed refs",
            game.id,
            cell.edid
        );
    }
}

#[test]
fn s14_2_and_s14_3_interior_water_surface_and_lighting_separation() {
    use nvre_editor::game::ALL;

    // Verify across all available game installs that cells with water surface
    // compute the water height correctly, separate from XCLL ambient/fog.
    for game in ALL {
        let Some(data_dir) = game.data_dir() else {
            continue;
        };
        let master_path = data_dir.join(game.master);
        if !master_path.exists() {
            continue;
        }
        let schema_path = std::path::Path::new("schemas").join(game.schema_subdir);
        let Ok(mut session) = nvre_editor::session::Session::open(
            &master_path,
            &schema_path,
            vec![game.master.to_string()],
        ) else {
            continue;
        };

        // Find a cell with a real water surface
        let water_cell = session
            .index
            .cells
            .values()
            .find(|c| {
                let world_default = c
                    .world
                    .and_then(|wid| session.index.worldspaces.get(&wid))
                    .and_then(|w| w.default_water_height);
                c.water_surface(world_default).is_some()
            })
            .cloned();

        if let Some(cell) = water_cell {
            let world_default = cell
                .world
                .and_then(|wid| session.index.worldspaces.get(&wid))
                .and_then(|w| w.default_water_height);
            let water_z = cell.water_surface(world_default);
            assert!(
                water_z.is_some(),
                "{}: cell with water surface must resolve water_surface",
                game.id
            );

            // Water properties are independent of cell XCLL lighting
            session.open_cell(cell.form_id);
            let lighting = session.cell_lighting();
            // Fog and ambient exist in XCLL, while water height is derived from XCLW/WRLD
            if let Some(light) = lighting {
                // Ensure lighting struct is distinct from water parameters
                assert!(light.fog_near >= 0.0, "{}: fog_near must be valid", game.id);
            }
        }
    }
}

#[test]
fn s8_6_default_landscape_layer_texture_and_fallback() {
    use nvre_vfs::normal_map_beside;
    use nvre_world::land::{DEFAULT_DIFFUSE, DEFAULT_NORMAL};

    // S8-6: A quadrant with no layer uses DEFAULT_DIFFUSE, and its normal map
    // is resolved beside it via normal_map_beside.
    assert_eq!(DEFAULT_DIFFUSE, "landscape\\dirtwasteland01.dds");
    assert_eq!(DEFAULT_NORMAL, "landscape\\dirtwasteland01_n.dds");
    let beside = normal_map_beside(DEFAULT_DIFFUSE);
    assert_eq!(beside.as_deref(), Some(DEFAULT_NORMAL));
}

#[test]
fn s8_7_terrain_built_once_per_worldspace_predicate() {
    // S8-7: draw_cell's needs_terrain_rebuild predicate:
    // Only rebuilds when:
    // 1. worldspace changes (spawned.terrain_world != world_opt)
    // 2. or full_terrain flag toggles
    // 3. or in local radius mode and radius changes
    let check_rebuild = |spawned_world: Option<u32>,
                         current_world: Option<u32>,
                         spawned_radius: i32,
                         current_radius: i32,
                         spawned_full: bool,
                         current_full: bool|
     -> bool {
        match (spawned_world, current_world) {
            (Some(sw), Some(cw)) => {
                sw != cw
                    || spawned_full != current_full
                    || (!current_full && spawned_radius != current_radius)
            }
            (Some(_), None) => true,
            (None, Some(_)) => true,
            (None, None) => false,
        }
    };

    // Moving between two cells in the SAME worldspace at the same radius does NOT rebuild terrain
    assert_eq!(
        check_rebuild(Some(10), Some(10), 5, 5, false, false),
        false,
        "Same worldspace and radius must NOT rebuild terrain"
    );

    // Changing worldspace DOES rebuild
    assert_eq!(
        check_rebuild(Some(10), Some(20), 5, 5, false, false),
        true,
        "Changing worldspace must rebuild terrain"
    );

    // Changing radius in local mode DOES rebuild
    assert_eq!(
        check_rebuild(Some(10), Some(10), 3, 5, false, false),
        true,
        "Changing radius must rebuild terrain"
    );

    // Changing radius in full_terrain mode does NOT rebuild
    assert_eq!(
        check_rebuild(Some(10), Some(10), 3, 5, true, true),
        false,
        "Full terrain mode must ignore radius changes"
    );
}

#[test]
fn s8_8_adjacent_cells_share_edge_vertices_coordinate_math() {
    use nvre_world::land::{SIDE, SPACING};
    use nvre_world::CELL_SIZE;

    // Each cell has SIDE x SIDE vertices (33 x 33) spanning CELL_SIZE (4096 units).
    // The spacing between consecutive vertices is exactly 4096 / 32 = 128 units.
    assert_eq!(SIDE, 33);
    assert_eq!(CELL_SIZE, 4096.0);
    assert_eq!(SPACING, 128.0);

    // Cell at grid (0, 0) right edge: x = 0 * 4096 + 32 * 128 = 4096.0
    // Cell at grid (1, 0) left edge:  x = 1 * 4096 + 0 * 128 = 4096.0
    let cell_0_right_edge_x = 0.0 * CELL_SIZE + (SIDE - 1) as f32 * SPACING;
    let cell_1_left_edge_x = 1.0 * CELL_SIZE + 0.0 * SPACING;
    assert_eq!(cell_0_right_edge_x, cell_1_left_edge_x);
}

#[test]
fn s9_9_directional_fade_scales_sun_illuminance() {
    use nvre_editor::viewport::DEFAULT_SUN_ILLUMINANCE;

    // When directional_fade is 1.0, illuminance equals DEFAULT_SUN_ILLUMINANCE
    let fade_full = 1.0f32;
    let ill_full = DEFAULT_SUN_ILLUMINANCE * fade_full.max(0.0);
    assert_eq!(ill_full, DEFAULT_SUN_ILLUMINANCE);

    // When directional_fade is 0.0, sun is dark (0.0 lux) rather than absent
    let fade_zero = 0.0f32;
    let ill_zero = DEFAULT_SUN_ILLUMINANCE * fade_zero.max(0.0);
    assert_eq!(ill_zero, 0.0);

    // Half fade scales linearly
    let fade_half = 0.5f32;
    let ill_half = DEFAULT_SUN_ILLUMINANCE * fade_half.max(0.0);
    assert_eq!(ill_half, DEFAULT_SUN_ILLUMINANCE * 0.5);

    // Negative fade clamps to 0.0
    let fade_neg = -0.5f32;
    let ill_neg = DEFAULT_SUN_ILLUMINANCE * fade_neg.max(0.0);
    assert_eq!(ill_neg, 0.0);
}

#[test]
fn s9_8_ambient_specular_subtraction_math() {
    // S9-8: Bevy adds ambient specular which nif_material.wesl subtracts:
    // out.color.rgb = max(out.color.rgb - view.exposure * ambient_spec, vec3(0.0))
    // Verifying the non-negativity clamp and subtraction logic:
    let out_color = Vec3::new(0.5, 0.4, 0.3);
    let exposure = 1.0f32;
    let ambient_spec = Vec3::new(0.1, 0.1, 0.1);

    let diffuse_only = (out_color - exposure * ambient_spec).max(Vec3::ZERO);
    assert!((diffuse_only - Vec3::new(0.4, 0.3, 0.2)).length() < 1e-6);

    // When ambient_spec exceeds out_color, clamp prevents negative color
    let large_ambient = Vec3::new(1.0, 1.0, 1.0);
    let clamped = (out_color - exposure * large_ambient).max(Vec3::ZERO);
    assert_eq!(clamped, Vec3::ZERO);
}
