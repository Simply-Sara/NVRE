//! Dragging an object out of the Objects window and into the viewport.
//!
//! GECK's Object Window and the Creation Kit's Object Window both work the same
//! way: a row in the object list is a drag source, the render window is a drop
//! target, and releasing over the world places a reference of that base object
//! where the cursor was. This is that, for the viewport.
//!
//! ## Why the drag is a Bevy message
//!
//! The two halves of the gesture live in different worlds. The **source** is a
//! table row in the Objects window, drawn by `panels::place` in that window's
//! egui schedule. The **target** is the viewport window, which carries no egui
//! context at all (see [`crate::windows::spawn_viewport`]). Neither can call the
//! other, so the gesture crosses as a [`DragStarted`] message: the source writes
//! it the frame egui reports the drag began, and the viewport reads it in
//! `Update` to remember what is being carried.
//!
//! Only the *start* needs the message. egui's own drag state stays the source of
//! truth for the rest of the gesture — a row that has begun a drag remains the
//! dragged widget until the pointer is released, and every window's pass sees
//! that — so [`dragged_object`] asks the Objects window's context which row is
//! being dragged each frame and the ghost is positioned from the viewport's own
//! cursor. Nothing is cached across frames, which is what makes a drag that
//! leaves and re-enters a window behave sanely.
//!
//! ## The two halves of a drop
//!
//! A drop has two answers, and they are deliberately separate:
//!
//! * [`drop_cell`] is where a drop *goes* — a ray through the cursor, tested
//!   against the loaded meshes, falling back to the ground plane. It is a pure
//!   function of the ray and the meshes, so a test can pin it without a window.
//! * [`crate::session::Session::place`] is what a drop *does* — it stages the
//!   REFR/ACHR/ACRE in the open cell with its NAME and its DATA and selects it.
//!
//! Only a drop inside the viewport window places anything. A drop over a panel
//! window is a no-op, and the ghost is not drawn there either, so the ghost and
//! the drop cannot disagree about whether they happened.

use crate::viewport::ViewportCamera;
use bevy::ecs::message::MessageReader;
use bevy::ecs::system::Query;
use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings, RayCastVisibility};
use bevy::prelude::*;
use bevy_egui::egui;

/// Whether the drag path logs its every state change.
///
/// Set `NVRE_DRAG_TRACE=1`. The drag is a chain of independent systems in
/// different windows' schedules, and a ghost that never appears gives no error
/// and no other log line — this is what makes each link observable: the drag
/// starting, being detected, the cursor being over the viewport or not, the
/// point found or missed, and the release being read or ignored.
pub fn trace() -> bool {
    std::env::var("NVRE_DRAG_TRACE").is_ok()
}

/// Log a drag state change, when [`trace`] is on.
///
/// `#[macro_export]` rather than `pub(crate) use`, because the panel is what
/// starts a drag and the viewport what finishes it: both need the same line
/// prefix for one log to read as one gesture.
#[macro_export]
macro_rules! trace_log {
    ($($arg:tt)*) => {
        if $crate::object_drag::trace() {
            bevy::log::info!("[drag] {}", format!($($arg)*));
        }
    };
}

/// What a drag out of the Objects window is carrying.
///
/// Public because it is both a Bevy message payload and the value
/// [`dragged_object`] reports; a `Local<T>` parameter makes its type part of the
/// system's signature, which the app registering the system has to name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DraggedObject {
    /// The base object's load-order FormID.
    pub form_id: u32,
    /// What the object is — `*b"STAT"` and so on.
    ///
    /// It travels with the FormID rather than being looked up at the far end
    /// because the *category* the row came from is already resolved, and a
    /// record's own signature is what decides the reference it places.
    pub sig: [u8; 4],
}

/// The id a drag's payload is filed under.
///
/// The id is per-context and lives only while the drag does, so the source
/// (which writes it) and the viewport (which reads it) share this one key rather
/// than two spellings of a string.
pub fn payload_id() -> egui::Id {
    egui::Id::new("NVRE_DRAGGED_OBJECT")
}

/// A drag out of the Objects window began.
///
/// Written by `panels::place` when egui reports the drag started, read by
/// [`begin_ghost`] to start drawing the ghost.
///
/// A message rather than egui's own payload because egui clears its payload the
/// moment the pointer is released — and the release is exactly the frame the
/// drop has to be read on.
#[derive(bevy::ecs::message::Message)]
pub struct DragStarted(pub DraggedObject);

/// Whether a drag out of the Objects window is in flight.
///
/// The drag is *not* detected by asking egui which widget it is dragging, which
/// is what this used to do and what made it fail. `egui_extras` applies a
/// table's [`egui::Sense`] to each **cell's child `Ui`**, whose id is derived from
/// `(row_index, col_index)` — so `Context::dragged_id()` is a cell id, never the
/// table's `id_salt`, and an `is_being_dragged(<table id>)` check can never be
/// true. The drag looked like it started and then ended on the very next frame.
///
/// Instead the gesture is driven by the two things the editor controls:
///
/// * [`DragStarted`], written by `panels::place` on the frame egui reports
///   `drag_started`, which begins the drag;
/// * the left button coming up, which ends it — either by placing (see
///   [`place_dropped_object`]) or by clearing the ghost, whichever applies.
///
/// A drag that egui itself aborts (Escape) is still ended by the button coming
/// up, because the button is the physical fact rather than egui's opinion.
pub fn dragged_object(ghost: &DragGhost) -> Option<DraggedObject> {
    ghost.object
}

// Whether the drop system has already reported the button as still held.
//
// A thread-local rather than a `Resource`: this is a flag on the drop system's
// own bookkeeping, not the editor's state.
thread_local! {
    static HELD_DOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The reference record a drop of this signature writes: an NPC places an `ACHR`,
/// a creature an `ACRE`, everything else a `REFR`.
///
/// The same mapping [`crate::session::Session::place`] applies, because the drop
/// goes through it; it is named here so the rule has one description.
pub fn placed_sig(sig: &[u8; 4]) -> [u8; 4] {
    match sig {
        b"NPC_" => *b"ACHR",
        b"CREA" => *b"ACRE",
        _ => *b"REFR",
    }
}

/// The viewport window's rect, in physical pixels, as egui sees it.
///
/// bevy_egui fills each context's `RawInput::screen_rect` from the camera's
/// physical viewport rect, so this is the coordinate space a ghost drawn by the
/// Objects window and a drop read by the viewport are both expressed in.
fn viewport_screen_rect(
    windows: &Query<&Window, With<crate::windows::ViewportWindow>>,
) -> Option<egui::Rect> {
    let window = windows.single().ok()?;
    let size = window.physical_size();
    Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(size.x as f32, size.y as f32),
    ))
}

/// Whether a cursor, in physical pixels, is over the viewport window.
///
/// The cursor position bevy_window reports is already `None` when the pointer is
/// outside that window, so this is the check for the frame the pointer crosses
/// the edge — the frame a drop has to be refused on.
pub fn cursor_over_viewport(
    windows: &Query<&Window, With<crate::windows::ViewportWindow>>,
    cursor: Vec2,
) -> bool {
    viewport_screen_rect(windows).is_some_and(|rect| rect.contains(egui::pos2(cursor.x, cursor.y)))
}

/// The world point a drop lands at, or `None` when the ray meets nothing.
///
/// The ray is tested against every mesh the scene has drawn — terrain and
/// references both — so releasing over a wall or a tabletop places the object on
/// the thing under the cursor rather than somewhere behind it.
///
/// A ray that meets no mesh still has an answer: it is intersected with the
/// ground plane, which is $z = 0$ in game space and therefore $y = 0$ in Bevy
/// space. That is the plane the game's own ground sits on, and it is what makes
/// a drop over bare sky land somewhere instead of nowhere. A ray parallel to the
/// ground, or one that meets it behind the camera, has no such meeting, and
/// `None` is the answer then.
///
/// A pure function of the ray and the meshes, which is what makes the placement
/// rule testable without a window: see `tests/object_drag.rs`.
pub fn drop_cell(ray: &bevy::shape::Ray3d, ray_cast: &mut MeshRayCast<'_, '_>) -> Option<Vec3> {
    // `Visible`, not `VisibleInView`: the latter is per-view state that is only
    // correct after a render pass has run, and a drop is read from input while
    // the frame is still being built. When it is stale the cast culls every mesh
    // and a drop over a wall silently lands on the ground behind it. `Visible`
    // asks the hierarchy, which is what the editor means by "drawn".
    let settings = MeshRayCastSettings {
        visibility: RayCastVisibility::Visible,
        ..default()
    };
    if let Some((_, hit)) = ray_cast.cast_ray(*ray, &settings).first() {
        return Some(hit.point);
    }
    ground_hit(ray)
}

/// Where a ray meets the ground plane ($y = 0$ in Bevy space), if it does.
///
/// `None` when the ray is parallel to the ground or meets it behind the camera —
/// a camera looking level, or looking up, has no ground point to offer.
pub fn ground_hit(ray: &bevy::shape::Ray3d) -> Option<Vec3> {
    let dir = *ray.direction;
    if dir.y.abs() <= 1e-6 {
        return None;
    }
    let t = -ray.origin.y / dir.y;
    (t > 0.0).then(|| ray.origin + dir * t)
}

/// The viewport camera's ray through a cursor position.
///
/// `viewport_to_world` wants the cursor in physical pixels relative to the
/// viewport rect, which is what [`crate::viewport::pick`] and the camera's orbit
/// anchor use too — so a ghost drawn from this ray and a selection made by a
/// click cannot disagree about where the cursor is.
pub fn cursor_ray(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    viewport_cursor: Vec2,
) -> Option<bevy::shape::Ray3d> {
    camera
        .viewport_to_world(camera_transform, viewport_cursor)
        .ok()
}

/// Bevy space → game space, as the viewport's own mapping does it.
///
/// [`crate::viewport::bevy_to_game`] already exists; it is named here because a
/// drop's whole job is that conversion, and a reader should not have to know
/// that "bevy_to_game" is the function that means it.
pub fn bevy_point_to_game(point: Vec3) -> [f32; 3] {
    crate::viewport::bevy_to_game(point).to_array()
}

/// The drag the viewport is currently drawing, and where it would land.
#[derive(Resource, Default)]
pub struct DragGhost {
    /// What is being dragged, when a drag is in progress.
    ///
    /// Set by [`begin_ghost`] from the source's message and cleared by
    /// [`update_ghost`] the frame no drag is reported — including the release
    /// frame, which is the one egui's payload is already gone from.
    pub object: Option<DraggedObject>,
    /// The point the ghost sits at, in Bevy space.
    pub point: Option<Vec3>,
    /// Whether the cursor is over the viewport window this frame.
    pub in_viewport: bool,
}

impl DragGhost {
    /// Whether a ghost should be drawn: something is being dragged, the cursor is
    /// over the viewport, and the ray found a landing point.
    pub fn is_showing(&self) -> bool {
        self.object.is_some() && self.in_viewport && self.point.is_some()
    }

    /// Forget the drag: nothing is being carried, and the ghost goes with it.
    ///
    /// The frame a drop is read on is also the frame the drag ends, so the
    /// release and the clearing are the same act rather than two that can drift.
    pub fn clear(&mut self) {
        self.object = None;
        self.point = None;
        self.in_viewport = false;
    }
}

/// Start drawing the ghost when a drag out of the Objects window begins.
pub fn begin_ghost(mut messages: MessageReader<DragStarted>, mut ghost: ResMut<DragGhost>) {
    for started in messages.read() {
        trace_log!(
            "begin_ghost: DragStarted for {:08X} ({}), ghost object {:?} -> {:?}",
            started.0.form_id,
            nvre_util::tag_str(&started.0.sig),
            ghost.object.map(|o| o.form_id),
            started.0.form_id
        );
        ghost.object = Some(started.0);
    }
}

/// Read the drag in progress and put the ghost where it would land.
///
/// Runs every frame, before [`draw_ghost`]. It clears the ghost when no drag is
/// reported — which is the frame the pointer is released, since egui's payload
/// is already gone by then — and otherwise casts the viewport's cursor ray and
/// asks [`drop_cell`] where it lands.
///
/// It places nothing. The release is read separately by
/// [`place_dropped_object`], which is the system that mutates the session.
#[allow(clippy::too_many_arguments)]
pub fn update_ghost(
    mut ghost: ResMut<DragGhost>,
    windows: Query<&Window, With<crate::windows::ViewportWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
    mut ray_cast: MeshRayCast<'_, '_>,
) {
    // A drag that has already been consumed is not a drag. `place_dropped_object`
    // runs after this and clears the ghost on the release frame, so by the time
    // this runs on the frame after the release the object is gone — which is what
    // stops the ghost from lingering for one frame too long.
    let object = match dragged_object(&ghost) {
        Some(o) => o,
        None => {
            if trace() && ghost.in_viewport {
                trace_log!("update_ghost: drag ended; clearing ghost");
            }
            ghost.clear();
            return;
        }
    };
    let Ok((camera, camera_transform)) = cameras.single() else {
        trace_log!("update_ghost: no viewport camera; ghost not positioned");
        return;
    };
    let Ok(window) = windows.single() else {
        trace_log!("update_ghost: no viewport window; ghost not positioned");
        return;
    };
    // The cursor bevy_window reports is already `None` when the pointer is
    // outside this window, so `None` here is the "cursor is over another window"
    // case: no ray, and the ghost says so.
    let Some(cursor_pos) = window.cursor_position() else {
        if trace() && ghost.in_viewport {
            trace_log!("update_ghost: cursor left the viewport window");
        }
        ghost.in_viewport = false;
        return;
    };
    let cursor_physical = cursor_pos * window.scale_factor();
    let over = cursor_over_viewport(&windows, cursor_physical);
    if trace() && over != ghost.in_viewport {
        trace_log!(
            "update_ghost: cursor {} the viewport (physical {:?})",
            if over { "entered" } else { "left" },
            cursor_physical
        );
    }
    ghost.in_viewport = over;
    if !over {
        // A drag in flight with the cursor over a panel window: the ghost is
        // deliberately not drawn there, and this line is what says so rather
        // than leaving the absence unexplained.
        trace_log!(
            "update_ghost: cursor at physical {cursor_physical:?} is outside the viewport; \
             no ghost this frame"
        );
    }
    let Some(ray) = cursor_ray(camera, camera_transform, cursor_physical) else {
        trace_log!("update_ghost: cursor ray did not convert; no landing point");
        return;
    };
    let point = drop_cell(&ray, &mut ray_cast);
    if trace() && point != ghost.point {
        trace_log!(
            "update_ghost: landing point {:?} (object {:08X})",
            point,
            object.form_id
        );
    }
    ghost.point = point;
}

/// The cursor the drag should show, which is the Objects window's to set.
///
/// `Grab` over the row says "this can be picked up"; `Grabbing` while the drag is
/// in flight says "it has been". The panel calls this for its own row response,
/// because the cursor is the one piece of the affordance that lives in egui's
/// output rather than in the viewport's.
pub fn drag_cursor(ghost: &DragGhost, over_viewport: bool) -> egui::CursorIcon {
    if ghost.object.is_none() {
        return egui::CursorIcon::Default;
    }
    if over_viewport {
        egui::CursorIcon::Grabbing
    } else {
        egui::CursorIcon::Grab
    }
}

/// Place the object a drag dropped, if the drop landed in the viewport.
///
/// The counterpart to [`update_ghost`]: it reads the same drag and acts only
/// when the left button came up this frame over the viewport window. Both use
/// the viewport's own cursor, so the ghost and the drop cannot disagree about
/// where the cursor was.
///
/// A drop over a panel window places nothing. The ghost was not drawn there, so
/// placing would be a drop the user never saw; the status line says which of the
/// two happened, because "the object vanished" and "the object landed on the
/// wall behind the cursor" look identical otherwise.
///
/// A plain click that never became a drag places nothing either: egui only
/// reports the table as dragged once the pointer has moved, so a released click
/// is not a drop.
#[allow(clippy::too_many_arguments)]
pub fn place_dropped_object(
    mut session: ResMut<crate::session::Session>,
    mut ghost: ResMut<DragGhost>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut ray_cast: MeshRayCast<'_, '_>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
    windows: Query<&Window, With<crate::windows::ViewportWindow>>,
) {
    // The ghost holds what was being carried: egui's payload is gone on the
    // release frame, so this is the only record left of the drag.
    let Some(object) = ghost.object else {
        return;
    };
    let released = buttons.just_released(MouseButton::Left);
    if !released {
        // Logged once per drag rather than per frame: this is the frame a drop is
        // *not* read on, which is every frame the button stays down.
        if trace() && !HELD_DOWN.with(|c| c.replace(true)) {
            trace_log!(
                "drop: object {:08X} carried, button still down",
                object.form_id
            );
        }
        return;
    }
    HELD_DOWN.with(|c| c.set(false));
    let Ok(window) = windows.single() else {
        trace_log!("drop: no viewport window; release ignored");
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        trace_log!("drop: cursor not over the viewport window; release ignored");
        ghost.clear();
        return;
    };
    let cursor_physical = cursor * window.scale_factor();
    if !cursor_over_viewport(&windows, cursor_physical) {
        trace_log!(
            "drop: release at physical {:?} is outside the viewport; nothing placed",
            cursor_physical
        );
        ghost.clear();
        session.status = "released outside the viewport; nothing placed".into();
        return;
    }
    let Ok((camera, camera_transform)) = cameras.single() else {
        trace_log!("drop: no viewport camera; release ignored");
        return;
    };
    let Some(ray) = cursor_ray(camera, camera_transform, cursor_physical) else {
        trace_log!("drop: cursor ray did not convert; nothing placed");
        return;
    };
    let Some(point) = drop_cell(&ray, &mut ray_cast) else {
        trace_log!("drop: no landing point under the cursor; nothing placed");
        ghost.clear();
        session.status = "nothing under the cursor to place on".into();
        return;
    };
    let game = bevy_point_to_game(point);
    trace_log!(
        "drop: placing {:08X} ({}) at bevy {point:?} -> game {:?}",
        object.form_id,
        nvre_util::tag_str(&object.sig),
        game
    );
    session.place(object.form_id, game);
    trace_log!(
        "drop: session status {:?}, {} reference staged",
        session.status,
        session.doc.inserted().len()
    );
    ghost.clear();
}

/// Draw the ghost: a wire box where the object would land, standing on the ground.
///
/// Not a real entity. The drop has not happened yet, and staging a reference
/// every frame while the button is held would fill the cell with records; what
/// the figure has to convey is *where* the object would land, and that is a
/// position, not a record.
///
/// The box's half-extent is a fixed 64 game units, the footprint of a mid-sized
/// static at the game's own scale. The object's real bounds are not known until
/// its model loads, and a box the right *size* is not what the figure is for.
pub fn draw_ghost(mut gizmos: Gizmos, ghost: Res<DragGhost>) {
    // `update_ghost` logs the *cause* of the ghost appearing or vanishing — it is
    // the system that decides it, and the cause is what distinguishes the exits
    // (nothing dragged / not over the viewport / no landing point). This system
    // draws only, so it stays silent: a per-frame line from a drawer would be
    // noise, and the decision is already reported.
    if !ghost.is_showing() {
        return;
    }
    let Some(point) = ghost.point else {
        return;
    };
    const HALF: f32 = 64.0;
    let colour = bevy::color::Color::srgba_u8(0x33, 0xCC, 0xFF, 200);
    // The base sits on the ground plane and the box stands on it: the ghost is
    // where the object's *feet* would be, which is how a placed reference reads.
    let base = Vec3::new(point.x, 0.0, point.z);
    trace_log!(
        "draw_ghost: drawing a box at base {base:?} (object {:08X})",
        ghost.object.map(|o| o.form_id).unwrap_or_default()
    );
    let corners = [
        base + Vec3::new(-HALF, 0.0, -HALF),
        base + Vec3::new(HALF, 0.0, -HALF),
        base + Vec3::new(HALF, 0.0, HALF),
        base + Vec3::new(-HALF, 0.0, HALF),
    ];
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        gizmos.line(a, b, colour);
        gizmos.line(a + Vec3::Y * HALF, b + Vec3::Y * HALF, colour);
        gizmos.line(a, a + Vec3::Y * HALF, colour);
    }
}
