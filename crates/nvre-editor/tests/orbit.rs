//! The camera's pose helpers must agree with each other.
//!
//! `place_camera` writes a pose and `camera_focus` reads one back. If the two
//! disagree, framing a reference puts the camera somewhere other than where the
//! debug window says, and the readout lies. These test the two together against a
//! real `PanOrbitCamera`, which is what the viewport drives.

use bevy::camera_controller::pan_orbit_camera::controller::component::PanOrbitCamera;
use bevy::prelude::*;
use nvre_editor::viewport::{camera_focus, place_camera, ViewPose, ViewportFocus};

/// A controller in the state the viewport spawns it in.
fn controller() -> PanOrbitCamera {
    PanOrbitCamera {
        last_anchor_depth: -(ViewportFocus::DEFAULT_DISTANCE as f64),
        ..default()
    }
}

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
        let mut cam = controller();
        place_camera(
            &mut transform,
            &mut cam,
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
fn a_pose_round_trips_through_the_camera() {
    // What `place_camera` writes, `camera_focus` must read back: the debug window
    // shows the second and the viewport applies the first, so a mismatch is a
    // readout that does not describe the view.
    let target = Vec3::new(512.0, 256.0, -1024.0);
    let distance = 4096.0;
    let mut transform = camera();
    let mut cam = controller();
    place_camera(
        &mut transform,
        &mut cam,
        ViewPose {
            target,
            distance,
            pitch: Some(-0.3),
            yaw: Some(1.1),
        },
    );

    let (read_target, read_distance) = camera_focus(&transform, &cam);
    assert!(
        read_target.distance(target) < 1.0,
        "the target must round-trip, got {read_target:?} vs {target:?}"
    );
    assert!(
        (read_distance - distance).abs() < 1.0,
        "the distance must round-trip, got {read_distance} vs {distance}"
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

    let mut cam = controller();
    place_camera(
        &mut transform,
        &mut cam,
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
    let mut cam = controller();
    place_camera(
        &mut above,
        &mut cam,
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
    let mut cam = controller();
    place_camera(
        &mut below,
        &mut cam,
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
fn placing_a_pose_resets_the_controllers_anchor() {
    // The controller remembers the depth of its last hit and orbits and zooms
    // about it. Left stale after the camera is moved, that anchor is a point that
    // no longer exists, and the next drag swings the view about it.
    let mut transform = camera();
    let mut cam = controller();
    cam.last_anchor_depth = -12.0;

    place_camera(
        &mut transform,
        &mut cam,
        ViewPose {
            target: Vec3::new(100.0, 0.0, 0.0),
            distance: 3000.0,
            pitch: Some(0.0),
            yaw: Some(0.0),
        },
    );

    assert!(
        (cam.last_anchor_depth().abs() - 3000.0).abs() < 1.0,
        "the anchor depth must follow the new distance, got {}",
        cam.last_anchor_depth()
    );
}
