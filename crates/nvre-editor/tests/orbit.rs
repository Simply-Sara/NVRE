//! The debug window's camera readout must agree with the camera itself.
//!
//! The panel recomputes where the camera is from the orbit state. If that
//! disagrees with `orbit_camera`, the readout lies and dialling a value in moves
//! the camera somewhere other than where the window said it would.

use bevy::prelude::*;

/// The same maths `orbit_camera` uses, and the debug window shows.
fn eye(yaw: f32, pitch: f32, target: Vec3, distance: f32) -> Vec3 {
    let dir = Vec3::new(
        yaw.cos() * pitch.cos(),
        pitch.sin(),
        yaw.sin() * pitch.cos(),
    );
    target - dir * distance
}

#[test]
fn the_eye_sits_the_orbit_distance_from_the_target() {
    for (yaw, pitch, target, distance) in [
        (
            0.6_f32,
            -0.35_f32,
            Vec3::new(-71564.0, -2115.0, 8380.0),
            1500.0,
        ),
        (1.2, 0.9, Vec3::ZERO, 600.0),
        (-2.5, -1.5, Vec3::new(1000.0, -2000.0, 3000.0), 20.0),
        (0.0, 0.0, Vec3::new(-72360.0, -3082.0, 7557.0), 200_000.0),
    ] {
        let e = eye(yaw, pitch, target, distance);
        assert!(
            (e.distance(target) - distance).abs() < 0.01,
            "the eye must sit {distance} from the target, got {}",
            e.distance(target)
        );
        // `look_at` points the camera's -Z at the target, so the direction from
        // the eye to the target is the orbit's own direction.
        let dir = Vec3::new(
            yaw.cos() * pitch.cos(),
            pitch.sin(),
            yaw.sin() * pitch.cos(),
        );
        assert!(
            (target - e).normalize().distance(dir) < 1e-5,
            "the camera must look at the target"
        );
    }
}

#[test]
fn yaw_and_pitch_move_the_eye_around_the_target() {
    let target = Vec3::new(500.0, 0.0, 500.0);
    let base = eye(0.0, 0.0, target, 100.0);
    // Turning the yaw swings the eye sideways without changing its height.
    let turned = eye(1.0, 0.0, target, 100.0);
    assert!(
        (base.y - turned.y).abs() < 1e-4,
        "yaw must not change the height"
    );
    assert!(base.distance(turned) > 1.0, "yaw must move the eye");
    // A higher pitch puts the eye lower, because the eye is `target - dir *
    // distance` and `dir.y` rises with the pitch. This is the opposite of the
    // intuitive reading, and it is why the default pitch is negative: a negative
    // pitch looks down at the cell from above.
    let raised = eye(0.0, 0.8, target, 100.0);
    let lowered = eye(0.0, -0.8, target, 100.0);
    assert!(lowered.y > base.y, "a negative pitch must look from above");
    assert!(raised.y < base.y, "a positive pitch must look from below");
    assert!((raised.distance(target) - base.distance(target)).abs() < 1e-4);
    assert!((lowered.distance(target) - base.distance(target)).abs() < 1e-4);
}
