//! The viewport's ambient light can be switched off from its hotbar.
//!
//! The ambient is the viewport's only light (`viewport::setup`), and the
//! hotbar's Ambient toggle is the only thing that turns it off. It is a
//! resource rather than an entity, so its brightness is the whole of the
//! switch: zero is off and [`DEFAULT_AMBIENT_BRIGHTNESS`] is on. This pins
//! that, and that the toggle is not one-way.

use bevy::prelude::*;
use nvre_editor::windows;

/// A minimal app: the hotbar, the ambient, and the toggle system.
///
/// The hotbar is registered **before** the first `update`: a system taking
/// `Res<ViewportHotbar>` fails validation and panics on the first frame if the
/// resource is not there yet, which is exactly the gap between a test app and
/// the real `App` builder.
fn app() -> App {
    let mut app = App::new();
    app.init_resource::<windows::ViewportHotbar>();
    app.add_systems(Startup, |mut commands: Commands| {
        commands.insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: windows::DEFAULT_AMBIENT_BRIGHTNESS,
            ..default()
        });
    });
    app.add_systems(Update, nvre_editor::windows::apply_ambient);
    app
}

/// The hotbar starts with the ambient on and no wireframe.
#[test]
fn the_hotbar_starts_with_the_ambient_on() {
    let mut app = app();
    app.update();
    let hotbar = app.world().resource::<windows::ViewportHotbar>();
    assert!(hotbar.ambient, "the ambient starts on");
    assert!(!hotbar.wireframe, "no wireframe at first");
    let ambient = app.world().resource::<GlobalAmbientLight>();
    assert!(
        (ambient.brightness - windows::DEFAULT_AMBIENT_BRIGHTNESS).abs() < f32::EPSILON,
        "the ambient is at the brightness the editor opens with"
    );
}

/// Turning the ambient off zeroes its brightness.
#[test]
fn turning_the_ambient_off_zeroes_it() {
    let mut app = app();
    app.update();
    {
        let mut hotbar = app.world_mut().resource_mut::<windows::ViewportHotbar>();
        hotbar.ambient = false;
    }
    app.update();
    let ambient = app.world().resource::<GlobalAmbientLight>();
    assert!(
        ambient.brightness.abs() < f32::EPSILON,
        "the ambient's brightness is zeroed, which is what takes it out of the frame"
    );
}

/// Turning the ambient back on restores the brightness it opened with, not zero.
#[test]
fn turning_the_ambient_back_on_restores_it() {
    let mut app = app();
    app.update();
    {
        let mut hotbar = app.world_mut().resource_mut::<windows::ViewportHotbar>();
        hotbar.ambient = false;
    }
    app.update();
    assert!(
        app.world()
            .resource::<GlobalAmbientLight>()
            .brightness
            .abs()
            < f32::EPSILON,
        "off"
    );
    {
        let mut hotbar = app.world_mut().resource_mut::<windows::ViewportHotbar>();
        hotbar.ambient = true;
    }
    app.update();
    let ambient = app.world().resource::<GlobalAmbientLight>();
    assert!(
        (ambient.brightness - windows::DEFAULT_AMBIENT_BRIGHTNESS).abs() < f32::EPSILON,
        "the ambient is restored to the value it opened with, not left at zero"
    );
}

/// The wireframe toggle leaves the ambient alone: the two are independent.
#[test]
fn the_wireframe_toggle_leaves_the_ambient_alone() {
    let mut app = app();
    app.update();
    {
        let mut hotbar = app.world_mut().resource_mut::<windows::ViewportHotbar>();
        hotbar.wireframe = true;
    }
    app.update();
    let ambient = app.world().resource::<GlobalAmbientLight>();
    assert!(
        (ambient.brightness - windows::DEFAULT_AMBIENT_BRIGHTNESS).abs() < f32::EPSILON,
        "the ambient is untouched by the wireframe toggle"
    );
}
