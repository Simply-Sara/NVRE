//! The camera's pose helpers must agree with each other.
//!
//! `place_camera` writes a pose and the pose read back off the `Transform` must
//! match it. If the two disagree, framing a reference puts the camera somewhere
//! other than where the debug window says, and the readout lies. These test the two
//! together.
//!
//! There is no controller and no focus point in either direction: `place_camera`
//! takes a target because that is where the editor has asked the camera to look,
//! and what is read back is the pose the camera is actually in. That is the whole
//! of the editor's camera state — a pose in, a pose out.

use bevy::prelude::*;
use nvre_editor::viewport::{place_camera, ViewPose, ViewportFocus};

/// A camera looking down -Z, as a fresh `Transform` does.
fn camera() -> Transform {
    Transform::IDENTITY
}

#[test]
fn placing_a_pose_puts_the_eye_the_distance_away() {
    for (target, distance) in [
        (Vec3::new(-71564.0, -2115.0, 8380.0), 1500.0_f32),
        (Vec3::ZERO, 600.0),
        (Vec3::new(1000.0, -2000.0, 3000.0), 20.0),
        (Vec3::new(-72360.0, -3082.0, 7557.0), 200_000.0),
    ] {
        let mut transform = camera();
        place_camera(
            &mut transform,
            ViewPose {
                target,
                distance,
                pitch: Some(-0.4),
                yaw: Some(0.9),
            },
        );

        assert!(
            (transform.translation.distance(target) - distance).abs() < 0.01,
            "the eye must sit {distance} from the target, got {}",
            transform.translation.distance(target)
        );
        // `look_at` points the camera's -Z at the target, so forward must point
        // from the eye towards the target.
        let to_target = (target - transform.translation).normalize();
        assert!(
            to_target.distance(*transform.forward()) < 1e-4,
            "the camera must look at the target, got forward {:?} vs {to_target:?}",
            *transform.forward()
        );
    }
}

#[test]
fn a_pose_round_trips_through_the_transform() {
    // What `place_camera` writes, the pose read back must describe: the debug
    // window shows the second and the viewport applies the first, so a mismatch is
    // a readout that does not describe the view.
    let target = Vec3::new(512.0, 256.0, -1024.0);
    let distance = 4096.0;
    let mut transform = camera();
    place_camera(
        &mut transform,
        ViewPose {
            target,
            distance,
            pitch: Some(-0.3),
            yaw: Some(1.1),
        },
    );

    let forward = *transform.forward();
    let read_target = transform.translation + forward * distance;
    assert!(
        read_target.distance(target) < 1.0,
        "the target must round-trip, got {read_target:?} vs {target:?}"
    );
    assert!(
        (transform.translation.distance(target) - distance).abs() < 1.0,
        "the distance must round-trip, got {}",
        transform.translation.distance(target)
    );
}

#[test]
fn a_pose_without_angles_keeps_the_heading() {
    // Framing a reference keeps the view's direction and only changes the aim, so
    // the camera swings round to the object instead of snapping to a fixed angle.
    let mut transform = camera();
    // Turn the camera to look along +X.
    transform.rotation = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
    let heading_before = *transform.forward();

    place_camera(
        &mut transform,
        ViewPose {
            target: Vec3::new(500.0, 0.0, 0.0),
            distance: 250.0,
            pitch: None,
            yaw: None,
        },
    );

    assert!(
        heading_before.distance(*transform.forward()) < 1e-4,
        "the heading must be kept, got {:?} vs {heading_before:?}",
        *transform.forward()
    );
    assert!(
        (transform.translation.distance(Vec3::new(500.0, 0.0, 0.0)) - 250.0).abs() < 0.01,
        "the eye must sit the requested distance from the target"
    );
}

#[test]
fn a_negative_pitch_looks_down_from_above() {
    // A cell opens with a negative pitch so an exterior is seen from above, the
    // way GECK frames a zone.
    let target = Vec3::new(0.0, 0.0, 0.0);
    let mut above = camera();
    place_camera(
        &mut above,
        ViewPose {
            target,
            distance: 1000.0,
            pitch: Some(-0.5),
            yaw: Some(0.0),
        },
    );
    assert!(
        above.translation.y > 0.0,
        "a negative pitch must put the eye above the target, got {:?}",
        above.translation
    );

    let mut below = camera();
    place_camera(
        &mut below,
        ViewPose {
            target,
            distance: 1000.0,
            pitch: Some(0.5),
            yaw: Some(0.0),
        },
    );
    assert!(
        below.translation.y < 0.0,
        "a positive pitch must put the eye below the target, got {:?}",
        below.translation
    );
}

#[test]
fn the_default_distance_is_the_one_a_cell_opens_at() {
    // Framing a reference and opening the cell it sits in have to put the camera
    // the same distance away, which is why there is one constant for both.
    assert_eq!(
        ViewportFocus::default().distance,
        ViewportFocus::DEFAULT_DISTANCE,
        "the default focus distance must be the cell-opening distance"
    );
}
