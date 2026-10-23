//! The drop system end to end, in a Bevy app.
//!
//! `tests/object_drag.rs` pins the pure rules; `tests/object_drop.rs` pins what a
//! drop writes. This drives the two systems that connect them — the ghost that
//! follows the cursor and the release that places — against a real window, a
//! real camera and a real mesh, so the wiring is checked rather than assumed.
//!
//! The one thing that cannot be driven here is egui's own drag detection, which
//! needs pointer events and a pass per frame. The test sets the same two facts
//! the panel sets (`is_being_dragged` on the table id, the payload in the context
//! data) and checks the systems read them.

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::window::WindowResolution;
use nvre_editor::object_drag::{
    begin_ghost, draw_ghost, place_dropped_object, update_ghost, DragGhost, DragStarted,
    DraggedObject,
};
use nvre_editor::session::Session;
use nvre_editor::viewport::ViewportCamera;
use nvre_editor::windows::ViewportWindow;

/// An app with the viewport's window, camera and one mesh in front of it.
fn app() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<DragGhost>()
        .add_message::<DragStarted>()
        .add_plugins(bevy::camera::visibility::VisibilityPlugin)
        // The gizmo mesh updater and the asset pipeline both read this; a
        // MinimalPlugins app has neither, so both are added here.
        .add_plugins(bevy::asset::AssetPlugin::default())
        // `update_skinned_mesh_bounds` runs with the visibility plugin and reads
        // this, so it has to exist even with no skinned meshes. `insert_resource`
        // rather than `init_resource`: the asset has no `FromWorld`.
        .insert_resource(Assets::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>::default())
        .insert_resource(Assets::<bevy::gizmos::GizmoAsset>::default())
        .add_plugins(bevy::transform::TransformPlugin)
        // The gizmos plugin registers the store and the default group, and
        // `Gizmos`/`draw_ghost` read both. `MinimalPlugins` does not, so the
        // plugin is added here rather than hand-registering each piece.
        .add_plugins(bevy::gizmos::GizmoPlugin)
        .add_systems(
            Update,
            (begin_ghost, update_ghost, place_dropped_object, draw_ghost).chain(),
        );

    let window = app
        .world_mut()
        .spawn((
            Window {
                resolution: WindowResolution::new(1280, 800),
                ..default()
            },
            ViewportWindow,
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
            RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
            // Looking along -Z from 250 units back, the way the viewport opens.
            Transform::from_xyz(0.0, 0.0, 250.0).looking_at(Vec3::ZERO, Vec3::Y),
            ViewportCamera,
        ))
        .id();

    // A quad at the origin, 200 units across, facing the camera: the "wall" a drop
    // should land on.
    let mut mesh = Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
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
    let wall = app
        .world_mut()
        .spawn((Mesh3d(handle), Transform::default(), Visibility::default()))
        .id();

    (app, camera, wall)
}

/// Put the cursor in the viewport window, where the drop reads it.
fn point_at(app: &mut App, window: Entity, position: Vec2) {
    let mut w = app.world_mut().get_mut::<Window>(window).unwrap();
    w.set_physical_cursor_position(Some(position.as_dvec2()));
}

#[test]
fn a_release_over_the_wall_places_on_the_wall() {
    let Some(mut session) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let cell = session
        .index
        .cells
        .values()
        .find(|c| c.edid.as_deref() == Some("Goodsprings"))
        .map(|c| c.form_id)
        .expect("Goodsprings");
    session.open_cell(cell);

    let (mut app, _camera, _wall) = app();
    app.insert_resource(session);
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<ViewportWindow>>()
        .single(app.world())
        .unwrap();

    let object = DraggedObject {
        form_id: 0x0000_0001,
        sig: *b"STAT",
    };
    app.world_mut().write_message(DragStarted(object));
    app.update();

    // The ghost has the object, so the drag is visible.
    assert!(
        app.world().resource::<DragGhost>().object.is_some(),
        "the message starts the ghost"
    );

    // Aim at the middle of the wall and release. The ray from the camera through
    // the window centre meets the quad at the origin, 250 units in front.
    point_at(&mut app, window, Vec2::new(640.0, 400.0));
    app.update();
    let ghost = app.world().resource::<DragGhost>();
    assert!(ghost.is_showing(), "the ghost is drawn");
    let point = ghost.point.expect("a landing point");
    assert!(
        point.z.abs() < 1.0 && (point.z + 250.0).abs() > 249.0,
        "the ghost sits on the wall, not the ground; got {point:?}"
    );

    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.update();

    let session = app.world().resource::<Session>();
    assert_eq!(
        session.doc.inserted().len(),
        1,
        "the release placed one reference"
    );
    let placed = &session.doc.inserted()[0].record;
    assert_eq!(placed.header.sig, *b"REFR");
    let subs = placed.subrecords().expect("subrecords");
    let data = subs.iter().find(|s| &s.sig == b"DATA").expect("DATA");
    let f = |i: usize| f32::from_le_bytes(data.data[i * 4..i * 4 + 4].try_into().unwrap());
    // Bevy's origin on the wall is game space (0, 0, 250), and the camera looks
    // along -Z so the drop's x is the cursor's x and its y is the cursor's y.
    assert!(
        (f(0) - 0.0).abs() < 1.0 && (f(1) - 0.0).abs() < 1.0,
        "the drop lands at the point the ghost was drawn at, not on the ground: {:?}",
        [f(0), f(1), f(2)]
    );
    // And the ghost is gone with the drag.
    assert!(
        app.world().resource::<DragGhost>().object.is_none(),
        "the release clears the ghost"
    );
}

#[test]
fn a_release_with_no_drag_places_nothing() {
    // The negative half: a plain click in the viewport must not place an object.
    // This is what stops every selection from dropping a reference.
    let Some(mut session) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let cell = session
        .index
        .cells
        .values()
        .find(|c| c.edid.as_deref() == Some("Goodsprings"))
        .map(|c| c.form_id)
        .expect("Goodsprings");
    session.open_cell(cell);

    let (mut app, _camera, _wall) = app();
    app.insert_resource(session);
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<ViewportWindow>>()
        .single(app.world())
        .unwrap();
    point_at(&mut app, window, Vec2::new(640.0, 400.0));
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.update();

    assert_eq!(
        app.world().resource::<Session>().doc.inserted().len(),
        0,
        "a click that never became a drag places nothing"
    );
}

#[test]
fn the_ghost_follows_the_cursor_across_the_wall() {
    // The ghost is where the object would land, so moving the cursor has to move
    // it. A ghost pinned to the screen centre would make every drop look the same
    // no matter where the cursor was.
    let (mut app, _camera, _wall) = app();
    if let Some(session) = open_master() {
        app.insert_resource(session);
    }
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<ViewportWindow>>()
        .single(app.world())
        .unwrap();
    app.world_mut().write_message(DragStarted(DraggedObject {
        form_id: 0x0000_0001,
        sig: *b"STAT",
    }));
    app.update();

    point_at(&mut app, window, Vec2::new(640.0, 400.0));
    app.update();
    let centre = app.world().resource::<DragGhost>().point.unwrap();

    point_at(&mut app, window, Vec2::new(1000.0, 400.0));
    app.update();
    let right = app.world().resource::<DragGhost>().point.unwrap();

    assert!(
        right.x > centre.x,
        "moving the cursor right moves the ghost right: {centre:?} then {right:?}"
    );
    // Both are on the wall, which is at z = -250 in Bevy space.
    assert!(centre.z.abs() < 1.0 && right.z.abs() < 1.0);
}

/// A session over `FalloutNV.esm`, when the environment names the game's data.
fn open_master() -> Option<Session> {
    let data = std::env::var("NVRE_DATA").ok()?;
    Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}
