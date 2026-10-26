//! The viewport camera has no pivot, and these tests pin that.
//!
//! This is the whole of NVRE's camera policy, and it is the opposite of what an
//! orbit camera does. A turn rotates the camera in place — the eye does not move —
//! and a wheel notch walks it along the line it is looking down.
//!
//! The cursor is aimed *off centre* on purpose throughout. A behaviour that only
//! differs at the screen centre is indistinguishable from one that ignores the
//! cursor entirely, so a test aimed at the middle would pass with a broken
//! implementation. A quarter of the way to the right edge is a position the
//! centre cannot be confused with.

use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowResolution};
use nvre_editor::viewport::{ViewportCamera, ViewportCameraPlugin};

/// An app with a window, a camera and a mesh in front of it.
fn app() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<MouseMotion>()
        .add_message::<MouseWheel>()
        .add_message::<bevy::input::touch::TouchInput>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<Assets<Mesh>>()
        .add_plugins(ViewportCameraPlugin);

    let window = app
        .world_mut()
        .spawn((
            Window {
                resolution: WindowResolution::new(1280, 800),
                ..default()
            },
            PrimaryWindow,
            nvre_editor::windows::ViewportWindow,
        ))
        .id();
    let _ = window;

    let camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                far: 1_500_000.0,
                near: 1.0,
                ..default()
            }),
            // Look at the mesh from 250 units back, along +Z.
            Transform::from_xyz(0.0, 0.0, 250.0).looking_at(Vec3::ZERO, Vec3::Y),
            ViewportCamera,
        ))
        .id();

    // A quad at the origin, 200 units across, facing the camera. It exists so the
    // scene is a real one: a turn with a mesh in front of the camera is the case
    // that would have been an orbit about it.
    let mut mesh = Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-100.0, -100.0, 0.0],
            [100.0, -100.0, 0.0],
            [100.0, 100.0, 0.0],
            [-100.0, 100.0, 0.0],
        ],
    );
    mesh.insert_indices(bevy::mesh::Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
    let target = app
        .world_mut()
        .spawn((Mesh3d(handle), Transform::default(), Visibility::default()))
        .id();

    (app, camera, target)
}

/// Put the cursor in the viewport window, where the input systems read it.
fn point_at(app: &mut App, window: Entity, position: Vec2) {
    let mut w = app.world_mut().get_mut::<Window>(window).unwrap();
    w.set_physical_cursor_position(Some(position.as_dvec2()));
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
    *app
        .world_mut()
        .query_filtered::<&Transform, With<ViewportCamera>>()
        .single(app.world())
        .expect("the camera must exist")
        .forward()
}

/// The viewport window.
fn viewport_window(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world())
        .unwrap()
}

#[test]
fn a_drag_turns_the_camera_and_leaves_the_eye_where_it_was() {
    let (mut app, _camera, _target) = app();
    let window = viewport_window(&mut app);

    // Off-centre on purpose. A turn must rotate the camera in place regardless of
    // where the pointer is, so a cursor over the mesh must not swing the eye
    // around it.
    point_at(&mut app, window, Vec2::new(800.0, 400.0));
    app.update();
    app.update();

    let before_eye = eye(&mut app);
    let before_forward = forward(&mut app);

    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Right);
    app.update();

    // A drag to the right, over several frames the way a mouse delivers it.
    for _ in 0..8 {
        app.world_mut().write_message(MouseMotion {
            delta: Vec2::new(20.0, 0.0),
        });
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
    }

    let during_eye = eye(&mut app);
    let during_forward = forward(&mut app);

    // The eye must not have moved at all: a turn is a rotation, not a swing. An
    // orbit about the view centre — or about the mesh under the cursor — would
    // have moved it.
    assert!(
        during_eye.distance(before_eye) < 1.0,
        "a turn must leave the eye where it was, moved {} units to {during_eye:?}",
        during_eye.distance(before_eye)
    );
    // The heading must have changed: a drag that moved nothing is not a turn.
    assert!(
        during_forward.distance(before_forward) > 0.01,
        "a turn must change the heading, stayed at {before_forward:?}"
    );
}

#[test]
fn a_wheel_notch_moves_the_camera_along_its_own_line_of_sight() {
    // Scrolling forward must walk the view towards whatever is in the middle of
    // the screen, not towards the mesh under the cursor. The camera here looks
    // down -Z, so a notch must move it along -Z and leave X and Y alone.
    let (mut app, _camera, _target) = app();
    let window = viewport_window(&mut app);

    // Off-centre again: a cursor at the centre would hide the difference.
    point_at(&mut app, window, Vec2::new(800.0, 400.0));
    app.update();
    app.update();

    let before = eye(&mut app);
    let before_forward = forward(&mut app);

    app.world_mut().write_message(MouseWheel {
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: 3.0,
        window: Entity::PLACEHOLDER,
        phase: bevy::input::touch::TouchPhase::Moved,
    });
    for _ in 0..30 {
        app.update();
    }

    let after = eye(&mut app);

    assert!(
        (after.z - before.z).abs() > 1.0,
        "a wheel notch must move the camera along its line of sight, stayed at {before:?}"
    );
    assert!(
        (after.x - before.x).abs() < 1.0 && (after.y - before.y).abs() < 1.0,
        "the wheel must not steer the camera sideways towards the cursor: {before:?} -> {after:?}"
    );
    assert!(
        forward(&mut app).distance(before_forward) < 1e-4,
        "the wheel must not turn the camera"
    );
}
