//! Where a drag out of the Objects window lands.
//!
//! The drag itself crosses windows and is carried by egui's own drag state, so
//! it is exercised through the pieces that can be pinned without a display:
//! which reference a drop writes, where a drop lands in the world, and where the
//! cursor has to be for a drop to count at all.
//!
//! The rule these pin is GECK's: release over the render window and the object is
//! placed where the cursor was, on top of whatever is under it.

use bevy::prelude::*;
use bevy::shape::Ray3d;
use nvre_editor::object_drag::{
    bevy_point_to_game, cursor_over_viewport, dragged_object, ground_hit, payload_id, placed_sig,
    DragGhost, DraggedObject,
};
use nvre_editor::windows::ViewportWindow;

/// A viewport window of the size the editor spawns it at.
fn viewport_window() -> Window {
    Window {
        resolution: (1280, 800).into(),
        ..default()
    }
}

#[test]
fn a_static_places_a_refr() {
    assert_eq!(placed_sig(b"STAT"), *b"REFR");
    assert_eq!(placed_sig(b"DOOR"), *b"REFR");
}

#[test]
fn an_npc_places_an_achr_and_a_creature_an_acre() {
    // The game reads an actor reference as a different record type, so a drop
    // that wrote a REFR over an NPC_ base would place an object the engine
    // refuses to spawn as an actor.
    assert_eq!(placed_sig(b"NPC_"), *b"ACHR");
    assert_eq!(placed_sig(b"CREA"), *b"ACRE");
}

#[test]
fn a_ray_looking_down_meets_the_ground_in_front_of_it() {
    let ray = Ray3d::new(Vec3::new(0.0, 500.0, 0.0), Dir3::new(Vec3::NEG_Y).unwrap());
    let hit = ground_hit(&ray).expect("a ray straight down meets the ground");
    assert!(
        hit.y.abs() < 1e-3,
        "the ground is the game's z = 0, which is Bevy's y = 0; got {hit:?}"
    );
}

#[test]
fn a_ray_looking_up_never_meets_the_ground() {
    // A camera pitched at the sky has no ground point, and a drop there must
    // say so rather than place the object at infinity.
    let ray = Ray3d::new(Vec3::new(0.0, 500.0, 0.0), Dir3::new(Vec3::Y).unwrap());
    assert!(ground_hit(&ray).is_none());
}

#[test]
fn a_ray_level_with_the_ground_has_no_meeting() {
    let ray = Ray3d::new(Vec3::new(0.0, 500.0, 0.0), Dir3::new(Vec3::X).unwrap());
    assert!(ground_hit(&ray).is_none());
}

#[test]
fn a_ray_meeting_the_ground_behind_the_camera_is_refused() {
    // The camera sits at y = -500 looking up, so the plane is "behind" it along
    // the ray. `t > 0` is what tells the two apart, and without it a drop from
    // below the world would land on the ground the camera has already passed.
    let ray = Ray3d::new(Vec3::new(0.0, -500.0, 0.0), Dir3::new(Vec3::NEG_Y).unwrap());
    assert!(ground_hit(&ray).is_none());
}

#[test]
fn an_angled_ray_lands_where_the_geometry_says_it_does() {
    // A camera above and to the side, looking down at the origin: the ground
    // meeting is where the ray's own equation puts it, not a rounded guess.
    let ray = Ray3d::new(
        Vec3::new(300.0, 500.0, 0.0),
        Dir3::new(Vec3::new(-300.0, -500.0, 0.0).normalize()).unwrap(),
    );
    let hit = ground_hit(&ray).expect("a ray towards the origin meets the ground");
    assert!((hit.x - 0.0).abs() < 1.0, "x: {}", hit.x);
    assert!((hit.y - 0.0).abs() < 1.0, "y: {}", hit.y);
}

#[test]
fn the_landing_point_is_read_back_into_game_space() {
    // Bevy is Y-up and the game is Z-up, and a drop writes a game position: the
    // y and z swap is the whole of `bevy_to_game`. Getting it wrong places every
    // dropped object a few thousand units above or below the world.
    let game = bevy_point_to_game(Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(game, [1.0, -3.0, 2.0]);
}

#[test]
fn the_dragged_object_carries_the_signature_it_places() {
    // The payload has to survive a round trip through egui's temp data, because
    // that is the only channel from the panel's pass to the viewport's frame.
    let object = DraggedObject {
        form_id: 0x00012345,
        sig: *b"STAT",
    };
    let mut ctx = bevy_egui::EguiContext::default();
    ctx.get_mut()
        .data_mut(|data| data.insert_temp(payload_id(), object));
    let back: Option<DraggedObject> = ctx.get_mut().data(|data| data.get_temp(payload_id()));
    assert_eq!(back, Some(object));
    assert_eq!(placed_sig(&back.unwrap().sig), *b"REFR");
}

#[test]
fn every_resource_the_drag_systems_read_is_registered() {
    // The regression this pins: `DragGhost` derives `Resource` but is only
    // usable once something calls `init_resource` on it, and a missing
    // registration is not a compile error — it is a runtime panic on the first
    // frame of every system that reads it. `cargo build` and `cargo check` both
    // pass without it, which is how the drag shipped broken.
    //
    // `main.rs` is a binary, so it cannot be called from a test. The check reads
    // the registration expression itself rather than a substring: the comment
    // above this one *contains* `init_resource::<DragGhost>()`, so a naive
    // substring search would pass on a comment and fail on real code.
    //
    // The turbofish is matched on its last path segment, so the app's own
    // qualification (`viewport::Spawned`, `object_drag::DragGhost`) satisfies the
    // check without the test having to enumerate module names.
    let main = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../nvre-editor/src/main.rs"
    ))
    .expect("main.rs");
    // Strip line comments so prose cannot satisfy the check.
    let code: String = main
        .lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect();
    // Every `init_resource::<…>()` argument, whitespace removed. A path is kept
    // to its last segment, which is what a bare name in the check is compared to.
    let registered: Vec<&str> = code
        .split("init_resource::<")
        .skip(1)
        .filter_map(|tail| tail.split(">()").next())
        .map(|ty| ty.rsplit("::").next().unwrap_or(ty))
        .collect();
    for resource in ["DragGhost", "Spawned", "GizmoState", "ObjectEditorState"] {
        assert!(
            registered.contains(&resource),
            "{resource} is read by a system but never registered in main.rs; \
             registered: {registered:?}"
        );
    }
}

#[test]
fn a_drag_in_the_ghost_is_reported() {
    // The detection is now the ghost's own state, not a query of egui's drag id:
    // `egui_extras` applies a table's sense to each cell's child `Ui`, so egui's
    // `dragged_id` is a cell id and can never equal the table's `id_salt`. This
    // pins the replacement — a `DragStarted` puts an object in the ghost, and the
    // reader reports it for as long as it is there.
    let ghost = DragGhost {
        object: Some(DraggedObject {
            form_id: 0x00ABCDEF,
            sig: *b"CONT",
        }),
        ..default()
    };
    assert_eq!(
        dragged_object(&ghost),
        Some(DraggedObject {
            form_id: 0x00ABCDEF,
            sig: *b"CONT",
        })
    );
}

#[test]
fn an_empty_ghost_reports_no_drag() {
    // The negative half: a session where nothing is being carried must not look
    // like a drag in progress, or every frame would draw a ghost.
    assert!(dragged_object(&DragGhost::default()).is_none());
}

#[test]
fn the_ghost_is_cleared_by_its_own_clear() {
    // `clear` is what ends a drag, and it has to remove the object rather than
    // leave it: a drag that ended but still reported an object would draw a ghost
    // over a world the user had already dropped into.
    let mut ghost = DragGhost {
        object: Some(DraggedObject {
            form_id: 0x00ABCDEF,
            sig: *b"CONT",
        }),
        point: Some(Vec3::new(1.0, 2.0, 3.0)),
        in_viewport: true,
    };
    ghost.clear();
    assert!(dragged_object(&ghost).is_none());
    assert!(ghost.point.is_none());
    assert!(!ghost.in_viewport);
    assert!(!ghost.is_showing());
}

#[test]
fn the_payload_round_trips_through_the_context() {
    // The payload is how the row's object crosses out of the panel's egui context
    // without a Bevy message per row. This is the channel the drag still uses for
    // the object itself; the ghost's state carries it across frames.
    let object = DraggedObject {
        form_id: 0x00012345,
        sig: *b"STAT",
    };
    let mut ctx = bevy_egui::EguiContext::default();
    ctx.get_mut()
        .data_mut(|data| data.insert_temp(payload_id(), object));
    let back: Option<DraggedObject> = ctx.get_mut().data(|data| data.get_temp(payload_id()));
    assert_eq!(back, Some(object));
    assert_eq!(placed_sig(&back.unwrap().sig), *b"REFR");
}

#[test]
fn the_cursor_is_over_the_viewport_only_inside_it() {
    // The viewport is the editor's second window, not the primary one, so a drop
    // test that used the primary window's rect would accept a release anywhere.
    let mut app = App::new();
    app.world_mut().spawn((viewport_window(), ViewportWindow));
    let mut state = bevy::ecs::system::SystemState::<
        bevy::ecs::system::Query<&Window, With<ViewportWindow>>,
    >::new(app.world_mut());
    let (w, h) = {
        let query = state.get(app.world()).unwrap();
        let window = query.single().expect("the viewport window");
        let size = window.physical_size();
        (size.x as f32, size.y as f32)
    };
    let mut state = bevy::ecs::system::SystemState::<
        bevy::ecs::system::Query<&Window, With<ViewportWindow>>,
    >::new(app.world_mut());
    assert!(cursor_over_viewport(
        &state.get(app.world()).unwrap(),
        Vec2::new(640.0, 400.0)
    ));
    let mut state = bevy::ecs::system::SystemState::<
        bevy::ecs::system::Query<&Window, With<ViewportWindow>>,
    >::new(app.world_mut());
    assert!(!cursor_over_viewport(
        &state.get(app.world()).unwrap(),
        Vec2::new(w + 1.0, 400.0)
    ));
    let mut state = bevy::ecs::system::SystemState::<
        bevy::ecs::system::Query<&Window, With<ViewportWindow>>,
    >::new(app.world_mut());
    assert!(!cursor_over_viewport(
        &state.get(app.world()).unwrap(),
        Vec2::new(640.0, h + 1.0)
    ));
}
