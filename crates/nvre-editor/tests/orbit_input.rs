//! The viewport's camera button mapping.
//!
//! The camera's own motion is the editor's, and which button starts which motion
//! is the whole of the policy. It is a pure function of the input state, so it can
//! be pinned here without a window, a camera, or a frame.
//!
//! The mapping is Bevy's own CAD example's: right-drag turns, middle-drag slides.
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
fn right_drag_turns() {
    let b = just_pressed(&[MouseButton::Right]);
    assert_eq!(motion_for(&b), Some(Motion::Turn));
}

#[test]
fn middle_drag_slides() {
    let b = just_pressed(&[MouseButton::Middle]);
    assert_eq!(motion_for(&b), Some(Motion::Slide));
}

#[test]
fn the_left_button_is_not_a_camera_button() {
    // Left-click selects and drags the gizmo. If it also moved the camera, every
    // selection would move the view.
    let b = just_pressed(&[MouseButton::Left]);
    assert_eq!(motion_for(&b), None);
}

#[test]
fn a_held_button_starts_nothing() {
    // `just_pressed` is what starts a motion: a button that is merely down is a
    // motion already in progress, and restarting it would drop the drag.
    let mut b = ButtonInput::<MouseButton>::default();
    b.press(MouseButton::Right);
    // A frame later the button is still down but no longer *just* pressed.
    b.clear();
    b.press(MouseButton::Right);
    assert_eq!(motion_for(&b), None);
}

#[test]
fn a_turn_ends_when_right_comes_up() {
    let mut b = ButtonInput::<MouseButton>::default();
    b.press(MouseButton::Right);
    assert!(motion_held(Motion::Turn, &b));
    b.release(MouseButton::Right);
    assert!(!motion_held(Motion::Turn, &b));
}

#[test]
fn a_slide_ends_when_middle_comes_up() {
    let mut b = ButtonInput::<MouseButton>::default();
    b.press(MouseButton::Middle);
    assert!(motion_held(Motion::Slide, &b));
    b.release(MouseButton::Middle);
    assert!(!motion_held(Motion::Slide, &b));
}

#[test]
fn one_motion_does_not_end_when_the_other_button_moves() {
    // The two motions have one button each. A slide must not end because the right
    // button was released, which is what a shared button would have caused.
    let mut b = ButtonInput::<MouseButton>::default();
    b.press(MouseButton::Middle);
    b.press(MouseButton::Right);
    b.release(MouseButton::Right);
    assert!(
        motion_held(Motion::Slide, &b),
        "releasing right must not end a middle-drag slide"
    );
    assert!(!motion_held(Motion::Turn, &b));
}

