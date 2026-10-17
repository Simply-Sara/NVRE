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
    Transform::from_translation(Vec3::new(position[0], -position[1], position[2]))
}

/// How `viewport::cell_centre` maps a game position.
fn cell_centre_one(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[2], p[1])
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
fn the_two_spaces_agree_on_which_way_is_right() {
    // Game +Z is "north". In both spaces it must map to the same Bevy axis, or a
    // horizontal drag turns the world one way and the camera the other.
    let a = [0.0_f32, 0.0, 0.0];
    let b = [0.0_f32, 0.0, 500.0]; // 500 units north
    let model_dir = ((game_to_bevy() * ref_transform(b)).translation
        - (game_to_bevy() * ref_transform(a)).translation)
        .normalize();
    let target_dir = (cell_centre_one(b) - cell_centre_one(a)).normalize();
    assert!(
        model_dir.distance(target_dir) < 1e-4,
        "game north is {model_dir:?} in model space but {target_dir:?} in target space"
    );

    // And the same for game +Y, which is the vertical axis in the file.
    let c = [0.0_f32, 500.0, 0.0];
    let model_y = ((game_to_bevy() * ref_transform(c)).translation
        - (game_to_bevy() * ref_transform(a)).translation)
        .normalize();
    let target_y = (cell_centre_one(c) - cell_centre_one(a)).normalize();
    assert!(
        model_y.distance(target_y) < 1e-4,
        "game up is {model_y:?} in model space but {target_y:?} in target space"
    );
}
