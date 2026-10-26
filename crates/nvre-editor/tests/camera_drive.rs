//! The viewport camera, driven the way a user drives it.
//!
//! The unit tests pin the maths and the button mapping. This one builds a real
//! `App` with the same plugin the editor adds, presses a button, feeds pointer
//! motion, and checks the camera actually did what the hand asked — which is the
//! part that cannot be checked by reading the code: whether the systems are wired
//! to each other in the right order.
//!
//! The behaviour pinned here is the free camera's whole point: a turn rotates the
//! camera in place, a slide translates it sideways, and a wheel notch walks it
//! along the line it is looking down. None of them has a pivot.

use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::WindowResolution;
use nvre_editor::viewport::{ViewportCamera, ViewportCameraPlugin};

/// An app with the camera plugin, one window and one camera, as the editor has.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<MouseMotion>()
        .add_message::<MouseWheel>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>();

    // The viewport window, which the camera renders to and the input is read from.
    app.world_mut().spawn((
        Window {
            resolution: WindowResolution::new(1280, 800),
            ..default()
        },
        nvre_editor::windows::ViewportWindow,
    ));

    app.world_mut().spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            far: 1_500_000.0,
            near: 1.0,
            ..default()
        }),
        // Look at the origin from 250 units back, along +Z.
        Transform::from_xyz(0.0, 0.0, 250.0).looking_at(Vec3::ZERO, Vec3::Y),
        ViewportCamera,
    ));

    app.add_plugins(ViewportCameraPlugin);
    app
}

/// The camera's translation.
fn eye(app: &mut App) -> Vec3 {
    app.world_mut()
        .query_filtered::<&Transform, With<ViewportCamera>>()
        .single(app.world())
        .expect("the camera must exist")
        .translation
}

/// The direction the camera is looking along.
fn forward(app: &mut App) -> Vec3 {
    *app.world_mut()
        .query_filtered::<&Transform, With<ViewportCamera>>()
        .single(app.world())
        .expect("the camera must exist")
        .forward()
}

/// Press a button, as a frame of input does.
fn press(app: &mut App, button: MouseButton) {
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(button);
}

/// Release a button.
fn release(app: &mut App, button: MouseButton) {
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(button);
}

/// Clear the per-frame "just pressed" state, as the input plugin does between
/// frames.
fn next_frame(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .clear();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
}

/// One frame of pointer motion.
fn move_pointer(app: &mut App, delta: Vec2) {
    app.world_mut().write_message(MouseMotion { delta });
}

/// One wheel notch.
fn wheel(app: &mut App, y: f32) {
    app.world_mut().write_message(MouseWheel {
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y,
        window: Entity::PLACEHOLDER,
        phase: bevy::input::touch::TouchPhase::Moved,
    });
}

/// The camera entity, for moving it by hand.
fn camera_entity(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<ViewportCamera>>()
        .single(app.world())
        .unwrap()
}

#[test]
fn a_right_drag_turns_the_view_in_place() {
    // A turn is a rotation: the eye stays where it is and the heading changes.
    // That is what makes this a free camera and not an orbit — an orbit would swing
    // the eye round a pivot, which is the behaviour this editor does not want.
    let mut app = app();

    let before_eye = eye(&mut app);
    let before_forward = forward(&mut app);

    press(&mut app, MouseButton::Right);
    app.update();

    for _ in 0..8 {
        move_pointer(&mut app, Vec2::new(20.0, 0.0));
        app.update();
        next_frame(&mut app);
    }

    let during_eye = eye(&mut app);
    let during_forward = forward(&mut app);

    assert!(
        during_eye.distance(before_eye) < 1.0,
        "a turn must not move the eye, moved {} units",
        during_eye.distance(before_eye)
    );
    assert!(
        during_forward.distance(before_forward) > 0.01,
        "a turn must change the heading, stayed at {before_forward:?}"
    );

    release(&mut app, MouseButton::Right);
    app.update();
}

#[test]
fn a_middle_drag_slides_the_camera() {
    // A slide is a translation in the camera's own plane: the eye moves, the
    // heading does not.
    let mut app = app();

    let before_eye = eye(&mut app);
    let before_forward = forward(&mut app);

    press(&mut app, MouseButton::Middle);
    app.update();

    for _ in 0..8 {
        move_pointer(&mut app, Vec2::new(0.0, 20.0));
        app.update();
        next_frame(&mut app);
    }

    let during_eye = eye(&mut app);
    assert!(
        before_eye.distance(during_eye) > 1.0,
        "a slide must move the camera, stayed at {before_eye:?}"
    );
    assert!(
        forward(&mut app).distance(before_forward) < 1e-4,
        "a slide must not change the heading"
    );

    release(&mut app, MouseButton::Middle);
    app.update();
}

#[test]
fn the_wheel_walks_along_the_line_of_sight() {
    // The wheel is a dolly, not a zoom: the camera moves along the line it is
    // looking down, by a fixed distance, with no sideways component and no scaling
    // with how far away it is.
    let mut app = app();

    let before_eye = eye(&mut app);
    let before_forward = forward(&mut app);

    wheel(&mut app, 1.0);
    for _ in 0..30 {
        app.update();
    }

    let after_eye = eye(&mut app);
    assert!(
        before_eye.distance(after_eye) > 1.0,
        "a wheel notch must move the camera, stayed at {before_eye:?}"
    );
    assert!(
        forward(&mut app).distance(before_forward) < 1e-4,
        "the wheel must not turn the camera"
    );
    // The camera looks down -Z here, so the walk is along -Z and only there.
    assert!(
        (after_eye.x - before_eye.x).abs() < 1.0 && (after_eye.y - before_eye.y).abs() < 1.0,
        "the walk must be along the line of sight: {before_eye:?} -> {after_eye:?}"
    );
}

#[test]
fn a_wheel_notch_moves_the_same_ground_at_any_distance() {
    // The bug this pins: a zoom is proportional to the distance it is zooming
    // along, so one notch at 16,000 units crosses 160 times the ground one does at
    // 1,000, and three notches end up inside the terrain. A dolly must move the
    // same step everywhere.
    fn notch_at(distance: f32) -> f32 {
        let mut app = app();
        let cam = camera_entity(&mut app);
        app.world_mut()
            .get_mut::<Transform>(cam)
            .unwrap()
            .translation = Vec3::new(0.0, 0.0, distance);
        // The focus sits at the same distance, which is what the slide's pixel
        // conversion reads.
        nvre_editor::viewport::request_view(
            &mut app.world_mut()
                .resource_mut::<nvre_editor::viewport::ViewportFocus>(),
            nvre_editor::viewport::ViewPose {
                target: Vec3::ZERO,
                distance,
                pitch: Some(0.0),
                yaw: Some(0.0),
            },
        );

        let before = eye(&mut app).z;
        wheel(&mut app, 1.0);
        for _ in 0..30 {
            app.update();
        }
        (before - eye(&mut app).z).abs()
    }

    let near = notch_at(1_000.0);
    let far = notch_at(16_000.0);
    assert!(
        near > 1.0 && far > 1.0,
        "a notch must move the camera at both distances, got {near} and {far}"
    );
    assert!(
        (near - far).abs() / far < 0.1,
        "one notch must move the same ground at any distance: {near} at 1,000 vs {far} at 16,000"
    );
}

#[test]
fn wasd_moves_the_camera_only_while_right_is_held() {
    // The flight keys are armed by a right-drag: hold right and W walks forward,
    // and let go and the same key does nothing. That is the whole rule, and both
    // halves of it are pinned here.
    let mut app = app();

    // Nothing held: W must not move the camera. The keys stay free for whatever
    // else wants them.
    let idle_before = eye(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyW);
    for _ in 0..10 {
        app.update();
        next_frame(&mut app);
    }
    assert!(
        idle_before.distance(eye(&mut app)) < 1.0,
        "WASD must not move the camera with nothing held, moved {} units",
        idle_before.distance(eye(&mut app))
    );

    // Right held: the same key walks. The flight is a per-second speed, so the
    // clock has to advance — the harness drives `Time` with wall-clock time, which
    // is what the sleeps are for.
    press(&mut app, MouseButton::Right);
    app.update();
    next_frame(&mut app);

    let armed_before = eye(&mut app);
    let before_forward = forward(&mut app);
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(16));
        app.update();
        next_frame(&mut app);
    }

    let armed_after = eye(&mut app);
    assert!(
        armed_before.distance(armed_after) > 1.0,
        "W must walk the camera while right is held, stayed at {armed_before:?}"
    );
    assert!(
        forward(&mut app).distance(before_forward) < 1e-4,
        "the flight keys must move the camera, not turn it"
    );

    release(&mut app, MouseButton::Right);
    app.update();
}

#[test]
fn wasd_does_not_move_the_camera_during_a_middle_drag() {
    // A middle-drag is a deliberate slide, and the flight keys are armed by a
    // right-drag only: pressing them during a slide must not add a second motion.
    let mut app = app();

    press(&mut app, MouseButton::Middle);
    app.update();
    next_frame(&mut app);

    let before = eye(&mut app);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyW);
    for _ in 0..10 {
        app.update();
        next_frame(&mut app);
    }

    assert!(
        before.distance(eye(&mut app)) < 1.0,
        "the flight keys must not move the camera during a middle-drag, moved {} units",
        before.distance(eye(&mut app))
    );

    release(&mut app, MouseButton::Middle);
    app.update();
}
