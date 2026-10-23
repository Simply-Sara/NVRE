//! Does the orbit actually anchor on the mesh under the cursor?
//!
//! This is what distinguishes an editor camera from a turntable: a drag starts an
//! orbit about the point under the pointer, so the thing you grabbed stays under
//! the pointer while the view swings around it.
//!
//! The anchor comes from a ray cast through the viewport camera. The cursor is
//! aimed *off centre* on purpose: an anchor at the screen centre is
//! indistinguishable from the controller's fallback (its last depth), so a test
//! aimed at the middle would pass even with a completely broken anchor.

use bevy::camera_controller::pan_orbit_camera::controller::component::PanOrbitCamera;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowResolution};
use nvre_editor::viewport::{ViewportCamera, ViewportCameraPlugin};

/// An app with a window, a camera and a mesh in front of it.
fn app() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<bevy::input::mouse::MouseMotion>()
        .add_message::<bevy::input::mouse::MouseWheel>()
        .add_message::<bevy::window::RequestRedraw>()
        .add_message::<bevy::window::WindowEvent>()
        .add_message::<bevy::input::touch::TouchInput>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>()
        // The picking plumbing: pointer input, the plugin, and the hover map that
        // fills in `PointerInteraction`.
        .add_plugins(bevy::camera_controller::pan_orbit_camera::DefaultPanOrbitCameraPlugins)
        .add_plugins(ViewportCameraPlugin)
        .add_plugins(bevy::camera::visibility::VisibilityPlugin)
        .add_plugins(bevy::transform::TransformPlugin);

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

    let camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                far: 1_500_000.0,
                near: 1.0,
                ..default()
            }),
            Camera {
                computed: bevy::camera::ComputedCameraValues {
                    clip_from_view: bevy::camera::Projection::Perspective(
                        PerspectiveProjection {
                            far: 1_500_000.0,
                            near: 1.0,
                            ..default()
                        },
                    )
                    .get_clip_from_view(),
                    target_info: Some(bevy::camera::RenderTargetInfo {
                        physical_size: UVec2::new(1280, 800),
                        scale_factor: 1.0,
                    }),
                    ..default()
                },
                ..default()
            },
            RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
            PanOrbitCamera {
                last_anchor_depth: -250.0,
                ..default()
            },
            // Look at the mesh from 250 units back, along +Z.
            Transform::from_xyz(0.0, 0.0, 250.0).looking_at(Vec3::ZERO, Vec3::Y),
            ViewportCamera,
        ))
        .id();

    // A quad at the origin, 200 units across, facing the camera.
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
    let handle = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(mesh);
    let target = app
        .world_mut()
        .spawn((
            Mesh3d(handle),
            Transform::default(),
            Visibility::default(),
        ))
        .id();

    (app, camera, target)
}

/// Put the cursor in the viewport window, where `cursor_anchor` reads it.
fn point_at(app: &mut App, window: Entity, position: Vec2) {
    let mut w = app.world_mut().get_mut::<Window>(window).unwrap();
    w.set_physical_cursor_position(Some(position.as_dvec2()));
}

#[test]
fn the_orbit_anchors_on_the_mesh_under_the_cursor() {
    let (mut app, camera, _target) = app();
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world())
        .unwrap();

    // Off-centre on purpose. An anchor at the screen centre is indistinguishable
    // from the controller's fallback (its last depth), so aiming at the middle
    // would let a completely broken picking path pass. A quarter of the way to
    // the right edge is a position the fallback can never produce.
    let cursor = Vec2::new(800.0, 400.0);
    point_at(&mut app, window, cursor);
    app.update();
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Right);
    app.update();

    let controller = app.world().get::<PanOrbitCamera>(camera).unwrap();
    let anchor = controller.anchor_view_space().expect("a drag must start with an anchor");

    // The quad is at the origin, 250 units in front of the camera, so the anchor
    // must be about that far away and off to the right, where the cursor was.
    assert!(
        anchor.z < -200.0 && anchor.z > -300.0,
        "the anchor must be on the quad, ~250 in front; got {anchor:?}"
    );
    assert!(
        anchor.x > 5.0,
        "the anchor must be to the right, where the cursor was, not on the centre line; got {anchor:?}"
    );
}