//! What does a rightward drag look like ON SCREEN?
//!
//! The question is not which way the eye moves, it is which way the world
//! appears to turn. Dragging right should make the scene spin as if you pushed it
//! right — the camera orbits to its own LEFT.
use bevy::prelude::*;

fn eye(yaw: f32, pitch: f32, distance: f32) -> Vec3 {
    let dir = Vec3::new(
        yaw.cos() * pitch.cos(),
        pitch.sin(),
        yaw.sin() * pitch.cos(),
    );
    -dir * distance
}

fn main() {
    let d = 250.0_f32;
    let target = Vec3::ZERO;

    // A rightward drag under the current code: yaw decreases.
    let start = eye(0.0, 0.0, d);
    let after = eye(-0.1, 0.0, d);

    // The camera's true basis. `look_at` points the camera's -Z at the target, so
    // forward = (target - eye).normalize(), and right = up × forward... or
    // forward × up? Print both and compare with a known point.
    let forward = (target - start).normalize();
    let right_a = forward.cross(Vec3::Y); // forward x up
    let right_b = Vec3::Y.cross(forward); // up x forward
    println!("forward      {forward:?}");
    println!("fwd x up     {right_a:?}");
    println!("up x fwd     {right_b:?}");

    // A marker 100 units along +Z from the target. At the start the camera is on
    // -X looking towards +X, so +Z is to the camera's... which side?
    let marker = Vec3::new(0.0, 0.0, 100.0);
    let to_marker = (marker - start).normalize();
    println!(
        "\nmarker at +Z: dot with fwd x up = {:.3}, dot with up x fwd = {:.3}",
        to_marker.dot(right_a),
        to_marker.dot(right_b)
    );
    println!("(the larger positive one is the camera's right)");

    // Now: dragging right moves the eye. Does the +Z marker appear to move left
    // or right on screen?
    let swing = (after - start).normalize();
    println!("\neye swing for a rightward drag: {swing:?}");
    println!("dot(swing, +Z) = {:.3}  (eye moves towards +Z)", swing.z);
    // The eye moving towards +Z means the camera turns to look more towards -Z,
    // so the +Z marker swings towards the camera's forward... i.e. to screen-left.
}
