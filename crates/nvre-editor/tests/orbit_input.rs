//! The viewport's camera button mapping.
//!
//! The controller owns everything about the camera except which button starts
//! which motion, and that mapping is the editor's. It is a pure function of the
//! input state, so it can be pinned here without a window, a camera, or a frame.
//!
//! The mapping is Bevy's own CAD example's: right-drag orbits, middle-drag pans.
//! Each motion has exactly one button, so nothing else can end it early and no
//! modifier has to be held.

use bevy::input::ButtonInput;
use bevy::prelude::*;
use nvre_editor::viewport::{motion_for, motion_held, Motion};

/// Inputs with the named buttons pressed *this frame*.
fn just_pressed(buttons: &[MouseButton]) -> ButtonInput<MouseButton> {
    let mut b = ButtonInput::<MouseButton>::default();
    for &button in buttons {
        b.press(button);
    }
    b
}

#[test]
fn right_drag_orbits() {
    let b = just_pressed(&[MouseButton::Right]);
    assert_eq!(motion_for(&b), Some(Motion::Orbit));
}

#[test]
fn middle_drag_pans() {
    let b = just_pressed(&[MouseButton::Middle]);
    assert_eq!(motion_for(&b), Some(Motion::Pan));
}

#[test]
fn the_left_button_is_not_a_camera_button() {
    // Left-click selects and drags the gizmo. If it also panned, every selection
    // would move the view.
    let b = just_pressed(&[MouseButton::Left]);
    assert_eq!(motion_for(&b), None);
}

#[test]
fn a_held_button_starts_nothing() {
    // `just_pressed` is what starts a motion: a button that is merely down is a
    // motion already in progress, and restarting it would drop the anchor.
    let mut b = ButtonInput::<MouseButton>::default();
    b.press(MouseButton::Right);
    // A frame later the button is still down but no longer *just* pressed.
    b.clear();
    b.press(MouseButton::Right);
    assert_eq!(motion_for(&b), None);
}

#[test]
fn an_orbit_ends_when_right_comes_up() {
    let mut b = ButtonInput::<MouseButton>::default();
    b.press(MouseButton::Right);
    assert!(motion_held(Motion::Orbit, &b));
    b.release(MouseButton::Right);
    assert!(!motion_held(Motion::Orbit, &b));
}

#[test]
fn a_pan_ends_when_middle_comes_up() {
    let mut b = ButtonInput::<MouseButton>::default();
    b.press(MouseButton::Middle);
    assert!(motion_held(Motion::Pan, &b));
    b.release(MouseButton::Middle);
    assert!(!motion_held(Motion::Pan, &b));
}

#[test]
fn one_motion_does_not_end_when_the_other_button_moves() {
    // The two motions have one button each. A pan must not end because the right
    // button was released, which is what a shared button would have caused.
    let mut b = ButtonInput::<MouseButton>::default();
    b.press(MouseButton::Middle);
    b.press(MouseButton::Right);
    b.release(MouseButton::Right);
    assert!(
        motion_held(Motion::Pan, &b),
        "releasing right must not end a middle-drag pan"
    );
    assert!(!motion_held(Motion::Orbit, &b));
}
