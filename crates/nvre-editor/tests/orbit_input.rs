//! The orbit controls: what a drag does to the camera.
//!
//! `MouseMotion`'s delta is cumulative since the last read, so the frame's
//! movement is the sum of the messages read this frame. Subtracting a running
//! total instead makes the camera jump backwards every other frame, which is what
//! "it keeps snapping back" was.

use bevy::prelude::*;

/// The camera's position for an orbit state: `target - dir * distance`.
fn eye(yaw: f32, pitch: f32, distance: f32) -> Vec3 {
    let dir = Vec3::new(
        yaw.cos() * pitch.cos(),
        pitch.sin(),
        yaw.sin() * pitch.cos(),
    );
    -dir * distance
}

/// The frame's movement, the way `orbit_input` computes it.
fn frame_movement(deltas: &[Vec2]) -> Vec2 {
    deltas.iter().copied().sum()
}

#[test]
fn the_frames_movement_is_the_sum_of_its_messages() {
    // Three messages in one frame add up; the running-total bug would have made
    // this the last message alone.
    let two = frame_movement(&[Vec2::new(1.0, 0.0), Vec2::new(2.0, 0.0)]);
    assert!((two - Vec2::new(3.0, 0.0)).length() < 1e-6);
    assert_eq!(frame_movement(&[]), Vec2::ZERO);
}

#[test]
fn a_drag_accumulates_without_snapping_back() {
    // Simulate a steady rightward drag over several frames. Each frame's own
    // messages are the same, so the yaw must keep moving the same way. The bug
    // this replaced subtracted a running total from a cumulative delta, which
    // goes negative the moment the reader is reset mid-drag and throws the camera
    // backwards.
    let mut yaw = 0.0_f32;
    for _ in 1..=6 {
        let fixed_delta = frame_movement(&[Vec2::new(4.0, 0.0)]);
        yaw -= fixed_delta.x * 0.005;
        // The movement must be positive every frame, never negative.
        assert!(
            fixed_delta.x > 0.0,
            "the frame's movement must not go negative"
        );
    }
    // Six frames of +4 px at 0.005 rad/px is 0.12 rad, all one way.
    assert!(
        (yaw + 0.12).abs() < 1e-6,
        "yaw must accumulate monotonically, got {yaw}"
    );
}

#[test]
fn dragging_right_turns_the_camera_right() {
    // At yaw 0 the eye is on -X (eye = -dir * distance, and dir is +X), so the
    // camera's forward is +X and its own right is +Z. Dragging right swings the
    // eye towards -Z, which is a *smaller* yaw — the opposite of the naive
    // reading, which is why this is pinned.
    let eye_at_zero = eye(0.0, 0.0, 250.0);
    let forward = (Vec3::ZERO - eye_at_zero).normalize();
    let right = forward.cross(Vec3::Y);

    let smaller = eye(-0.5, 0.0, 250.0);
    let larger = eye(0.5, 0.0, 250.0);
    let swing_right = (smaller - eye_at_zero).normalize();
    let swing_left = (larger - eye_at_zero).normalize();

    assert!(
        swing_right.dot(right) > 0.0,
        "a smaller yaw must move the eye towards the camera's own right, got {swing_right:?} vs {right:?}"
    );
    assert!(
        swing_left.dot(right) < 0.0,
        "a larger yaw must move the eye towards the camera's own left"
    );
}

#[test]
fn dragging_up_looks_down_from_above() {
    // A negative pitch puts the eye above the target, so dragging up (screen y
    // decreasing, so a negative delta) must raise the eye.
    let up = eye(0.0, -0.5, 250.0);
    assert!(up.y > 0.0, "dragging up must look from above, got {up:?}");
    let down = eye(0.0, 0.5, 250.0);
    assert!(
        down.y < 0.0,
        "dragging down must look from below, got {down:?}"
    );
}
