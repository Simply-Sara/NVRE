//! The viewport camera, driven the way a user drives it.
//!
//! The unit tests pin the maths and the button mapping. This one builds a real
//! `App` with the same plugin the editor adds, presses a button, feeds pointer
//! motion, and checks the camera actually moved and actually stopped — which is
//! the part that cannot be checked by reading the code: whether the systems are
//! wired to each other in the right order.

use bevy::camera_controller::pan_orbit_camera::controller::component::PanOrbitCamera;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::picking::pointer::{PointerId, PointerLocation, PointerMap};
use bevy::prelude::*;
use bevy::window::WindowResolution;
use nvre_editor::viewport::{ViewportCamera, ViewportCameraPlugin};

/// An app with the camera plugin, one window and one camera, as the editor has.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<MouseMotion>()
        .add_message::<MouseWheel>()
        // The controller asks for a redraw whenever it moves the camera; the
        // window plugin normally provides this.
        .add_message::<bevy::window::RequestRedraw>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<PointerMap>()
        // `MeshRayCast` (the cursor anchor) reads this.
        .init_resource::<Assets<Mesh>>()
        .add_plugins(bevy::camera_controller::pan_orbit_camera::DefaultPanOrbitCameraPlugins)
        .add_plugins(ViewportCameraPlugin);

    // The window the pointer is in, and the camera rendering to it.
    let window = app
        .world_mut()
        .spawn(Window {
            resolution: WindowResolution::new(1280, 800),
            ..default()
        })
        .id();
    let _ = window;

    app.world_mut().spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            far: 1_500_000.0,
            near: 1.0,
            ..default()
        }),
        // The controller computes pan and orbit distances from the camera's
        // logical viewport size, and zoom from its clip-from-view matrix. Both
        // are filled in by the render plugin, which is not in this harness, so
        // they are set directly: without them every motion silently does nothing,
        // which is what made this harness report a camera that never moved.
        Camera {
            computed: bevy::camera::ComputedCameraValues {
                clip_from_view: bevy::camera::Projection::Perspective(PerspectiveProjection {
                    far: 1_500_000.0,
                    near: 1.0,
                    ..default()
                })
                .get_clip_from_view(),
                target_info: Some(bevy::camera::RenderTargetInfo {
                    physical_size: UVec2::new(1280, 800),
                    scale_factor: 1.0,
                }),
                ..default()
            },
            ..default()
        },
        PanOrbitCamera {
            last_anchor_depth: -(nvre_editor::viewport::ViewportFocus::DEFAULT_DISTANCE as f64),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 250.0).looking_at(Vec3::ZERO, Vec3::Y),
        ViewportCamera,
    ));

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

#[test]
fn a_right_drag_orbits_the_camera() {
    // The pointer has to exist for the picking map, even with no backend: the
    // anchor is looked up by pointer id.
    let mut app = app();
    let pointer = app
        .world_mut()
        .spawn((PointerId::Mouse, PointerLocation::default()))
        .id();
    let _ = pointer;

    let before = eye(&mut app);

    press(&mut app, MouseButton::Right);
    app.update();

    // A drag to the right, over several frames the way a mouse delivers it.
    for _ in 0..8 {
        move_pointer(&mut app, Vec2::new(20.0, 0.0));
        app.update();
        next_frame(&mut app);
    }

    let during = eye(&mut app);
    assert!(
        before.distance(during) > 1.0,
        "a right drag must move the camera, stayed at {before:?}"
    );
    // An orbit keeps the distance to what it is looking at: the eye swings round
    // rather than flying off.
    let centre = Vec3::ZERO;
    assert!(
        (during.distance(centre) - before.distance(centre)).abs() < before.distance(centre) * 0.25,
        "an orbit must keep its distance, went from {} to {}",
        before.distance(centre),
        during.distance(centre)
    );

    release(&mut app, MouseButton::Right);
    app.update();
}

#[test]
fn the_camera_stops_when_the_button_comes_up() {
    // Momentum means it coasts briefly, but it must settle: a camera that never
    // stops is a camera that cannot be left alone. The coast decays with real
    // time, so this runs the clock forward rather than counting frames.
    let mut app = app();
    app.world_mut()
        .spawn((PointerId::Mouse, PointerLocation::default()));

    press(&mut app, MouseButton::Right);
    app.update();
    move_pointer(&mut app, Vec2::new(40.0, 0.0));
    app.update();
    next_frame(&mut app);

    release(&mut app, MouseButton::Right);
    app.update();

    // Let any momentum decay. Five simulated seconds is well past the point a
    // flick should have stopped.
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(16));
        app.update();
    }
    let settled = eye(&mut app);
    std::thread::sleep(std::time::Duration::from_millis(200));
    app.update();
    let later = eye(&mut app);
    assert!(
        settled.distance(later) < 1.0,
        "the camera must come to rest, drifted {} units",
        settled.distance(later)
    );
}

#[test]
fn a_middle_drag_pans_the_camera() {
    let mut app = app();
    app.world_mut()
        .spawn((PointerId::Mouse, PointerLocation::default()));

    let before = eye(&mut app);
    press(&mut app, MouseButton::Middle);
    app.update();

    for _ in 0..8 {
        move_pointer(&mut app, Vec2::new(0.0, 20.0));
        app.update();
        next_frame(&mut app);
    }

    let during = eye(&mut app);
    assert!(
        before.distance(during) > 1.0,
        "a middle drag must pan the camera, stayed at {before:?}"
    );

    release(&mut app, MouseButton::Middle);
    app.update();
}

#[test]
fn wasd_does_not_move_the_camera() {
    // The flight keys were removed on purpose: the controller's own orbit is the
    // whole camera, and WASD was a second way to move it that fought the first.
    let mut app = app();
    app.world_mut()
        .spawn((PointerId::Mouse, PointerLocation::default()));

    let before = eye(&mut app);
    press(&mut app, MouseButton::Right);
    app.update();

    for key in [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD] {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        next_frame(&mut app);
    }

    let after = eye(&mut app);
    assert!(
        before.distance(after) < 1.0,
        "WASD must not move the camera, it moved {} units",
        before.distance(after)
    );
}

#[test]
fn the_wheel_zooms_without_a_button_held() {
    // A wheel notch with nothing pressed is a zoom, which is how a viewport
    // behaves: you do not hold a button to zoom.
    let mut app = app();
    app.world_mut()
        .spawn((PointerId::Mouse, PointerLocation::default()));

    let before = eye(&mut app).distance(Vec3::ZERO);

    app.world_mut().write_message(MouseWheel {
        unit: bevy::input::mouse::MouseScrollUnit::Line,
        x: 0.0,
        y: 3.0,
        window: Entity::PLACEHOLDER,
        phase: bevy::input::touch::TouchPhase::Moved,
    });
    for _ in 0..30 {
        app.update();
    }

    let after = eye(&mut app).distance(Vec3::ZERO);
    assert!(
        (after - before).abs() > 1.0,
        "the wheel must zoom, distance stayed {before}"
    );
}

#[test]
fn the_wheel_stops_zooming_when_the_wheel_stops() {
    // The bug this pins: a zoom whose input is only sent on wheel events never
    // decays, because the smoothing queue's last smoothed sample stays constant
    // and is applied every frame. The camera then scrolls for ever, which is what
    // scrolling while an orbit is held looked like.
    let mut app = app();
    app.world_mut()
        .spawn((PointerId::Mouse, PointerLocation::default()));

    app.world_mut().write_message(MouseWheel {
        unit: bevy::input::mouse::MouseScrollUnit::Line,
        x: 0.0,
        y: 3.0,
        window: Entity::PLACEHOLDER,
        phase: bevy::input::touch::TouchPhase::Moved,
    });
    // A burst of frames, then no more wheel input at all.
    for _ in 0..20 {
        app.update();
    }
    let settled = eye(&mut app).distance(Vec3::ZERO);
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(16));
        app.update();
    }
    let later = eye(&mut app).distance(Vec3::ZERO);
    assert!(
        (settled - later).abs() < 1.0,
        "the camera must stop zooming once the wheel stops, drifted {} units",
        (settled - later).abs()
    );
}

#[test]
fn scrolling_while_orbiting_stops_when_the_wheel_stops() {
    // The reported bug: hold right (orbiting), scroll, release the wheel. The
    // zoom must end with the wheel, not continue for as long as the button is
    // held.
    let mut app = app();
    app.world_mut()
        .spawn((PointerId::Mouse, PointerLocation::default()));

    press(&mut app, MouseButton::Right);
    app.update();
    next_frame(&mut app);

    app.world_mut().write_message(MouseWheel {
        unit: bevy::input::mouse::MouseScrollUnit::Line,
        x: 0.0,
        y: 2.0,
        window: Entity::PLACEHOLDER,
        phase: bevy::input::touch::TouchPhase::Moved,
    });
    for _ in 0..20 {
        app.update();
    }
    // Still holding right, but no longer scrolling.
    let settled = eye(&mut app).distance(Vec3::ZERO);
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(16));
        app.update();
    }
    let later = eye(&mut app).distance(Vec3::ZERO);
    assert!(
        (settled - later).abs() < 1.0,
        "the zoom must stop when the wheel stops, even with right held; drifted {} units",
        (settled - later).abs()
    );
}

#[test]
fn one_wheel_notch_is_a_visible_step() {
    // "The zoom is too slow" is a complaint about magnitude, so the amount is
    // pinned: a single notch at the editor's default distance must move the view
    // by a noticeable fraction of that distance, not by a rounding error. The
    // scale is Bevy's CAD example's 150-per-line, which is what makes a notch
    // feel like a step rather than a crawl in a world measured in game units.
    let mut app = app();
    app.world_mut()
        .spawn((PointerId::Mouse, PointerLocation::default()));

    let before = eye(&mut app).distance(Vec3::ZERO);
    app.world_mut().write_message(MouseWheel {
        unit: bevy::input::mouse::MouseScrollUnit::Line,
        x: 0.0,
        y: 1.0,
        window: Entity::PLACEHOLDER,
        phase: bevy::input::touch::TouchPhase::Moved,
    });
    for _ in 0..30 {
        app.update();
    }

    let after = eye(&mut app).distance(Vec3::ZERO);
    let moved = (after - before).abs();
    assert!(
        moved > before * 0.02,
        "one notch must move the view by more than 2% of {before}, moved {moved}"
    );
}
