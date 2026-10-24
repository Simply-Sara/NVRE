//! The viewport: a 3D view of the open cell's placed references, with an orbit
//! camera and click-to-select.
//!
//! The editor's own state lives in [`crate::session`]; this module only turns it
//! into entities and reads clicks back. Meshes are spawned through NVRE's
//! `nvre-assets` layer, so a reference looks the way it does in the game rather
//! than as a placeholder box.
//!
//! Game space is Z-up with one unit ≈ 4.19 cm; Bevy space is Y-up. The conversion
//! is the same one the game itself uses (`world::game_to_bevy`).
//!
//! A reference's model is placed by the asset layer's own root, which already
//! carries the NIF's Z-up-to-Bevin-Y-up rotation; the reference's transform is
//! composed onto that rather than replacing it.
//!
//! ## The camera
//!
//! The viewport's camera is Bevy's own `PanOrbitCamera`. It owns the pose: a drag
//! is a motion it is told to start and then fed screen-space deltas, and it moves
//! the `Transform` itself. That is why nothing here keeps a yaw/pitch/distance
//! alongside the camera — the editor asks the camera where it is
//! ([`sync_focus`]) and, when something has to be looked at, writes a pose
//! ([`place_camera`]). [`ViewportFocus`] is the one piece of intent the editor
//! keeps: what is worth looking at, and from how far.

use bevy::asset::RenderAssetUsages;
use bevy::camera_controller::pan_orbit_camera::controller::component::PanOrbitCamera;
use bevy::camera_controller::pan_orbit_camera::controller::motion::CurrentMotion;
use bevy::ecs::message::MessageReader;
use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings, RayCastVisibility};
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use nvre_assets::model::ModelCtx;
use nvre_world::land::{
    land_texture, load_land, Land, LandParts, DEFAULT_DIFFUSE, QUAD_SIDE, SIDE, SPACING,
};
use nvre_world::transform::ref_rotation;
use std::collections::{HashMap, HashSet};

/// Marks the viewport's orbit camera.
#[derive(Component)]
pub struct ViewportCamera;

/// An entity standing for one placed reference.
#[derive(Component)]
pub struct RefEntity {
    pub form_id: u32,
    pub bounds: Option<(Vec3, Vec3)>,
}

/// Game space → Bevy space, as the game does it.
pub fn game_to_bevy() -> Transform {
    Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
}

/// Bevy-space position → game-space position.
///
/// Used to read a camera's position back into game units, which is where a
/// placed reference's DATA stores it.
#[allow(dead_code)]
pub fn bevy_to_game(p: Vec3) -> Vec3 {
    Vec3::new(p.x, -p.z, p.y)
}

/// The reference's transform in Bevy space.
pub fn ref_transform(position: [f32; 3], rotation: [f32; 3], scale: f32) -> Transform {
    let m = Mat3::from_cols_array(&ref_rotation(rotation)).transpose();
    Transform {
        translation: Vec3::from_array(position),
        rotation: Quat::from_mat3(&m),
        scale: Vec3::splat(scale),
    }
}

/// What the viewport is looking at, as the editor thinks of it.
///
/// The camera itself owns its pose; this is the intent behind it. `target` is
/// where a cell opening or a frame request wants the view, and `distance` is how
/// far away it wants to be. It is written by [`crate::panels::debug`] and read
/// back by [`sync_focus`] once the user has moved the camera, so the debug window
/// shows where the camera actually is rather than where it was last told to go.
#[derive(Resource)]
pub struct ViewportFocus {
    /// What the camera looks at, in Bevy space.
    pub target: Vec3,
    /// How far from the target the camera sits.
    pub distance: f32,
    /// A view waiting to be applied, if anything asked for one.
    ///
    /// Cleared by [`sync_focus`] once the camera is there. Without it the camera
    /// would be yanked back to `target`/`distance` every frame the user moved it,
    /// which is what makes a camera fight the hand on the mouse.
    pending: Option<ViewPose>,
}

impl ViewportFocus {
    /// How far from the target the camera opens, and how far it frames a
    /// reference at.
    ///
    /// One constant, because these have to agree: framing a reference and
    /// framing the cell it sits in should put the camera the same distance away.
    pub const DEFAULT_DISTANCE: f32 = 250.0;
}

impl Default for ViewportFocus {
    fn default() -> Self {
        ViewportFocus {
            target: Vec3::ZERO,
            distance: Self::DEFAULT_DISTANCE,
            pending: None,
        }
    }
}

/// A view the editor has asked for: what to look at, from how far, and optionally
/// from which angle.
///
/// The angles are optional because the two callers want different things. Framing
/// a reference keeps the view's heading and levels it off — the camera swings
/// round to the object rather than snapping to a fixed angle. Opening a cell sets
/// both, because a cell is entered from a known angle: level in an interior, and
/// looking down at an exterior from above.
#[derive(Clone, Copy)]
pub struct ViewPose {
    pub target: Vec3,
    pub distance: f32,
    /// Elevation of the camera above the target, in radians. Negative looks down
    /// at the target from above.
    pub pitch: Option<f32>,
    /// Heading around the target, in radians.
    pub yaw: Option<f32>,
}

/// Which models and terrain have been spawned, so a frame does not respawn them.
#[derive(Resource, Default)]
pub struct Spawned {
    pub refs: HashMap<u32, Entity>,
    pub terrain: Vec<Entity>,
    /// Which worldspace the currently spawned terrain belongs to.
    pub terrain_world: Option<u32>,
    pub full_terrain: bool,
    pub cell: Option<u32>,
    pub radius: i32,
    /// Whether base object model paths have been batch-resolved for the current cell/radius.
    pub bases_resolved: bool,
    /// Cached base FormID -> model path for the current cell / zone.
    pub model_paths: HashMap<u32, Option<String>>,
    /// Models that have been loaded into ModelCtx.
    pub loaded_models: HashSet<String>,
    /// References that could not be drawn, so they are not retried every frame.
    failed: HashSet<u32>,
}

/// Spawn the viewport's light and ambient light.
///
/// The camera itself is spawned by [`crate::windows::spawn_viewport`], which is
/// what ties it to the viewport window. Spawning a second camera here would
/// leave one with no window to render to, and its egui context's output would
/// never be consumed.
pub fn setup(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 12000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.8, -0.9, 0.0)),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.7, 0.75, 0.8),
        brightness: 800.0,
        ..default()
    });
}

/// Keep the camera where the focus says it is.
///
/// The camera controller owns the pose between moves, so this only acts when the
/// editor has actually asked for a view: [`ViewportFocus`] holds a pending
/// [`ViewPose`], and it is cleared here once the camera has been put there.
/// Reading the pose back afterwards is what keeps the debug window honest — it
/// describes where the camera is, not the last thing that was requested.
pub fn sync_focus(
    mut cameras: Query<(&mut Transform, &mut PanOrbitCamera), With<ViewportCamera>>,
    mut focus: ResMut<ViewportFocus>,
) {
    let Ok((mut transform, mut controller)) = cameras.single_mut() else {
        return;
    };
    if let Some(pose) = focus.pending.take() {
        place_camera(&mut transform, &mut controller, pose);
    }
    // A camera in the middle of a move is where the user put it a frame ago; the
    // pose it is heading for is the next frame's business.
    if controller.current_motion.is_moving() {
        return;
    }
    let (target, distance) = camera_focus(&transform, &controller);
    focus.target = target;
    focus.distance = distance;
}

/// Put the camera at a pose.
///
/// The heading is kept unless the pose names one — only the distance and the aim
/// change — so a view that is already turned towards an object swings round to it
/// rather than snapping to a fixed angle. The controller's anchor depth is reset
/// with it, because a controller that remembers the depth of its last hit would
/// otherwise orbit about a point that is no longer there.
pub fn place_camera(transform: &mut Transform, controller: &mut PanOrbitCamera, pose: ViewPose) {
    // The camera sits opposite the direction it looks along, so the pose's
    // direction of view is the negation of where the camera goes.
    let direction = match (pose.yaw, pose.pitch) {
        (yaw, pitch) if yaw.is_some() || pitch.is_some() => {
            let yaw = yaw.unwrap_or(0.0);
            let pitch = pitch.unwrap_or(0.0);
            Vec3::new(
                yaw.cos() * pitch.cos(),
                pitch.sin(),
                yaw.sin() * pitch.cos(),
            )
        }
        _ => *transform.forward(),
    };
    transform.translation = pose.target - direction * pose.distance;
    transform.look_at(pose.target, Vec3::Y);
    controller.current_motion = CurrentMotion::Stationary;
    controller.last_anchor_depth = -(pose.distance.abs() as f64);
}

/// Where a camera is looking and from how far, read off its own pose.
///
/// The distance is the controller's own anchor depth, which is what it orbits and
/// zooms about: after a wheel-zoom or a pan that is the only record of how far the
/// view is from what it is looking at.
pub fn camera_focus(transform: &Transform, controller: &PanOrbitCamera) -> (Vec3, f32) {
    let distance = controller.last_anchor_depth().abs() as f32;
    let distance = if distance.is_finite() && distance > 0.0 {
        distance
    } else {
        ViewportFocus::DEFAULT_DISTANCE
    };
    let forward = transform.forward();
    (transform.translation + *forward * distance, distance)
}

/// Ask for a view.
///
/// The one way anything moves the camera: a cell opening, a frame request and the
/// debug window all go through this, so there is a single place where "look at
/// this from here" is expressed.
pub fn request_view(focus: &mut ViewportFocus, pose: ViewPose) {
    focus.target = pose.target;
    focus.distance = pose.distance;
    focus.pending = Some(pose);
}

/// The viewport's camera input: which button starts which motion, and the
/// screen-space deltas that drive it.
///
/// The controller itself has no opinion about buttons — it is told to start an
/// orbit, a pan or a zoom, fed deltas while the motion lasts, and told when it
/// ends. This is that telling, and it is the whole of the editor's camera
/// handling: the pose, the smoothing, the momentum, the clamping and the orbit
/// constraint are Bevy's.
///
/// The anchor for an orbit or a zoom is the point under the cursor, taken from
/// real mesh hits — orbiting about a wall you clicked on keeps that wall under the
/// pointer instead of swinging the view about the screen centre. When the cursor
/// is over empty space the controller falls back to the depth of its last hit, so
/// dragging from the sky still orbits about something sensible.
pub struct ViewportCameraPlugin;

impl Plugin for ViewportCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraDrag>()
            .init_resource::<ViewportFocus>()
            .add_systems(
                PreUpdate,
                (
                    start_camera_motions,
                    feed_camera_input,
                    end_camera_motions,
                    end_zoom_motion,
                )
                    .chain()
                    // After picking has produced its hits for this frame, and
                    // before the controller consumes them. The controller's own
                    // system runs in `SyncCameraPosition`, which is already
                    // ordered after picking, so this is the window between.
                    .after(bevy::picking::PickingSystems::Last)
                    .before(PanOrbitCamera::update_camera_positions),
            );
        // `frame_selected` and `sync_focus` are registered by the binary, in the
        // `Editing` state: the first reads the session, which does not exist
        // before a plugin is open, and a system whose parameters fail validation
        // fails the schedule.
    }
}

/// Which camera motion a button starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Motion {
    Orbit,
    Pan,
}

/// The button that starts each motion.
///
/// Right-drag orbits, middle-drag pans, and the wheel zooms. This is the mapping
/// Bevy's own CAD example uses, and the reason for it: an orbit is the motion a
/// hand reaches for constantly and the right button is the one under it, while a
/// pan is deliberate and gets the middle button, which nothing else wants.
///
/// Public because it is the whole of the editor's own camera policy; everything
/// else about the camera is the controller's. It is a pure function of the input
/// state so the mapping can be pinned without a window.
pub fn motion_for(buttons: &ButtonInput<MouseButton>) -> Option<Motion> {
    if buttons.just_pressed(MouseButton::Right) {
        return Some(Motion::Orbit);
    }
    if buttons.just_pressed(MouseButton::Middle) {
        return Some(Motion::Pan);
    }
    None
}

/// Whether the motion a button started is still held.
///
/// One button each, so a motion ends when its own button comes up and nothing
/// else can end it early.
pub fn motion_held(motion: Motion, buttons: &ButtonInput<MouseButton>) -> bool {
    match motion {
        Motion::Orbit => buttons.pressed(MouseButton::Right),
        Motion::Pan => buttons.pressed(MouseButton::Middle),
    }
}

/// How much a wheel notch of each unit is worth to the controller.
///
/// The controller treats zoom input as a number it multiplies by the size of a
/// pixel at the anchor, so the scale here is what decides how fast the wheel
/// feels. 150 is Bevy's own CAD example's figure for a line-based wheel, and it is
/// the one that makes a notch a visible step rather than a crawl: NVRE's anchor
/// depths are in game units (thousands of them), where a multiplier of 1 moves the
/// camera by a fraction of a unit and the view looks stuck.
fn scroll_scale(unit: MouseScrollUnit) -> f32 {
    match unit {
        MouseScrollUnit::Line => 150.0,
        MouseScrollUnit::Pixel => 1.0,
    }
}

/// Which motion is in progress.
///
/// A resource rather than a `Local`, because the input, motion and end systems are
/// separate and all need it.
#[derive(Resource, Default)]
pub struct CameraDrag {
    motion: Option<Motion>,
}

/// Start a motion on a button press, anchored at the mesh under the cursor.
fn start_camera_motions(
    mut drag: ResMut<CameraDrag>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut ray_cast: MeshRayCast,
    windows: Query<&Window, With<crate::windows::ViewportWindow>>,
    mut controllers: Query<(&mut PanOrbitCamera, &Camera, &GlobalTransform), With<ViewportCamera>>,
) {
    let Some(motion) = motion_for(&buttons) else {
        return;
    };
    let Ok((mut controller, camera, camera_transform)) = controllers.single_mut() else {
        return;
    };
    // A motion already in flight is not restarted: the controller debounces that
    // itself, and starting one would drop the anchor it is orbiting about.
    if controller.is_actively_controlled() {
        return;
    }
    let anchor = cursor_anchor(&mut ray_cast, &windows, camera, camera_transform);
    match motion {
        Motion::Orbit => controller.start_orbit(anchor),
        Motion::Pan => controller.start_pan(anchor),
    }
    drag.motion = Some(motion);
}

/// The point under the cursor, in the camera's view space.
///
/// A ray is cast from the viewport camera through the viewport window's cursor and
/// the nearest mesh it meets is the anchor. That is what makes a drag orbit about
/// the thing under the pointer rather than the screen centre.
///
/// This casts the ray itself rather than reading `bevy_picking`'s
/// `PointerInteraction`. In a multi-window editor that component is per *pointer*,
/// not per camera, and what it holds for the mouse is dominated by the window
/// entity the pointer is over — its `position` is the cursor in screen
/// coordinates, not a world point. Filtering that out leaves nothing unless the
/// mesh backend happened to contribute a hit for *this* camera, and when it does
/// not, the controller silently falls back to its last depth and orbits the centre
/// of the screen. A ray cast from this camera has neither problem: it is scoped to
/// the viewport, it cannot see another window's geometry, and a miss is a miss.
fn cursor_anchor(
    ray_cast: &mut MeshRayCast,
    windows: &Query<&Window, With<crate::windows::ViewportWindow>>,
    camera: &Camera,
    camera_transform: &GlobalTransform,
) -> Option<bevy::math::DVec3> {
    let window = windows.single().ok()?;
    let cursor = window.cursor_position()?;
    let ray = camera.viewport_to_world(camera_transform, cursor).ok()?;
    // `Visible`, not `VisibleInView`. `ViewVisibility` is set by the render
    // plugin for entities a camera is actually drawing, which makes it the more
    // precise filter — but it is per-view state that is only correct after a
    // render pass has run, and an anchor is read while the input is handled. When
    // it is stale or unset the cast culls every mesh and the orbit silently
    // anchors on the screen centre instead of the cursor. `Visible` asks the
    // hierarchy, which is what the editor means by "the thing under the pointer".
    let settings = MeshRayCastSettings {
        visibility: RayCastVisibility::Visible,
        ..default()
    };
    let (_, hit) = ray_cast.cast_ray(ray, &settings).first()?;
    Some(
        camera_transform
            .affine()
            .inverse()
            .transform_point3(hit.point)
            .as_dvec3(),
    )
}

/// Feed the frame's movement and wheel to the controller.
///
/// The wheel is handled whether or not a drag is in progress, because zooming is
/// the one motion that does not need a button held: the controller is told to
/// start a zoom-only motion, given the wheel, and the motion is left to lapse.
fn feed_camera_input(
    drag: Res<CameraDrag>,
    mut ray_cast: MeshRayCast,
    windows: Query<&Window, With<crate::windows::ViewportWindow>>,
    mut controllers: Query<(&mut PanOrbitCamera, &Camera, &GlobalTransform), With<ViewportCamera>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
) {
    let Ok((mut controller, camera, camera_transform)) = controllers.single_mut() else {
        return;
    };

    // The wheel, and then the zoom input *every* frame — including a zero when
    // there was no wheel this frame. The controller's zoom is a smoothed input
    // queue: `send_zoom_input` is what pushes a new sample, and the smoothed
    // velocity it applies is the average over the queue's window. Sending only on
    // wheel events leaves the last sample as the newest entry, so the queue keeps
    // returning the same non-zero velocity and the camera zooms for ever — the
    // "it keeps scrolling" bug. A zero every frame is what lets the average fall
    // back to nothing.
    let mut zoomed = 0.0;
    for w in wheel.read() {
        zoomed += w.y * scroll_scale(w.unit);
    }

    if zoomed != 0.0 && drag.motion.is_none() {
        // A wheel notch with no drag in progress starts a zoom-only motion,
        // anchored at the cursor: zooming towards the thing under the pointer is
        // the whole point of a wheel in an editor, and `None` would zoom towards
        // the centre of the screen instead.
        let anchor = cursor_anchor(&mut ray_cast, &windows, camera, camera_transform);
        controller.start_zoom(anchor);
    }
    // Every motion that can zoom gets a sample every frame, zero included.
    //
    // An orbit drag is `OrbitZoom` and a pan is `PanZoom`, so neither
    // `is_zooming_only` nor a "no drag" test alone covers it: a wheel turned
    // during an orbit feeds the *orbit's* zoom queue, and that queue decays by the
    // same mechanism. `send_zoom_input` is a no-op unless the camera is under
    // user control, so sending unconditionally is safe and is what Bevy's own
    // input plugin does.
    controller.send_zoom_input(zoomed);

    if drag.motion.is_none() {
        // Read and drop, so a drag from before this motion does not arrive as one
        // huge delta the next time a button is pressed.
        motion.clear();
        return;
    }
    // `MouseMotion`'s delta is cumulative since the last read, so the frame's
    // movement is the sum of the messages read this frame.
    let screenspace: Vec2 = motion.read().map(|m| m.delta).sum();
    controller.send_screenspace_input(screenspace);
}

/// Let a zoom-only motion lapse once the wheel has stopped.
///
/// A zoom started by the wheel has no button to release, so it is ended when its
/// own input has been consumed. Without this the controller would stay in a
/// zooming motion for ever and the next orbit would be interpreted as a zoom.
fn end_zoom_motion(
    drag: Res<CameraDrag>,
    mut controllers: Query<&mut PanOrbitCamera, With<ViewportCamera>>,
) {
    if drag.motion.is_some() {
        return;
    }
    let Ok(mut controller) = controllers.single_mut() else {
        return;
    };
    if !controller.current_motion.is_zooming_only() {
        return;
    }
    // The queue still holds samples for the smoothing window; once the smoothed
    // velocity has decayed to nothing the zoom is over.
    let still_zooming = controller
        .current_motion
        .inputs()
        .map(|inputs| inputs.zoom_velocity_abs(controller.smoothing.zoom.mul_f32(2.0)))
        .unwrap_or(0.0)
        > 0.0;
    if !still_zooming {
        controller.end_move();
    }
}

/// End the motion when the button that started it comes up.
fn end_camera_motions(
    mut drag: ResMut<CameraDrag>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut controllers: Query<&mut PanOrbitCamera, With<ViewportCamera>>,
) {
    let Some(motion) = drag.motion else {
        return;
    };
    if motion_held(motion, &buttons) {
        return;
    }
    if let Ok(mut controller) = controllers.single_mut() {
        controller.end_move();
    }
    drag.motion = None;
}

/// `F` frames the selection, the way GECK's `T` does.
pub fn frame_selected(
    keys: Res<ButtonInput<KeyCode>>,
    session: Res<crate::session::Session>,
    refs: Query<(&RefEntity, &GlobalTransform)>,
    mut focus: ResMut<ViewportFocus>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Some(fid) = session.selected else {
        return;
    };
    frame_reference(&mut focus, &refs, fid);
}

/// Draw the open cell's references, spawning models as they are first seen.
///
/// Only the cell the session has open is drawn; a mod is edited one cell at a
/// time, and a whole worldspace is far more entities than an editor frame needs.
pub fn draw_cell(
    mut commands: Commands,
    mut session: ResMut<crate::session::Session>,
    mut spawned: ResMut<Spawned>,
    mut models: ModelCtx,
    filters: Res<crate::panels::Filters>,
    mut focus: ResMut<ViewportFocus>,
) {
    let Some(cell) = session.cell else { return };
    let (is_exterior, world_opt, grid_opt) = {
        let cell_entry = session.index.cells.get(&cell);
        (
            cell_entry.is_some_and(|c| c.grid.is_some()),
            cell_entry.and_then(|c| c.world),
            cell_entry.and_then(|c| c.grid),
        )
    };
    let radius = if is_exterior {
        if filters.exterior_radius < 0 {
            -1
        } else {
            filters.exterior_radius.clamp(0, 10)
        }
    } else {
        0
    };

    if spawned.cell != Some(cell) || spawned.radius != radius {
        // A different cell or view radius: drop spawned references.
        for (_, e) in spawned.refs.drain() {
            commands.entity(e).despawn();
        }
        spawned.failed.clear();
        spawned.model_paths.clear();
        spawned.bases_resolved = false;
        spawned.cell = Some(cell);
        spawned.radius = radius;
        // And look at the new cell: the camera's target stays where it was, so
        // without this it looks at the last cell's centre and the new one is off
        // screen.
        let refs = session.cell_refs().to_vec();
        if let Some(centre) = cell_centre(&refs) {
            let (distance, pitch) = if is_exterior {
                let d = match radius {
                    0 => 2000.0,
                    1 => 3500.0,
                    2 => 5500.0,
                    3 => 8000.0,
                    -1 => 25000.0,
                    _ => 12000.0,
                };
                (d, -0.5)
            } else {
                (ViewportFocus::DEFAULT_DISTANCE, 0.0)
            };
            request_view(
                &mut focus,
                ViewPose {
                    target: centre,
                    distance,
                    pitch: Some(pitch),
                    yaw: Some(0.0),
                },
            );
        }
    }

    // Terrain management:
    // Terrain is persistent across cells within the same worldspace.
    // It only rebuilds when switching worldspaces, when full_terrain toggles,
    // when radius changes in local mode, or when switching between interior and exterior.
    let needs_terrain_rebuild = match (world_opt, spawned.terrain_world) {
        (None, Some(_)) => true, // moved to interior: clear terrain
        (Some(w), Some(tw)) => {
            w != tw
                || spawned.full_terrain != filters.full_terrain
                || (!filters.full_terrain && spawned.radius != radius)
        }
        (Some(_), None) => true, // opened exterior cell: spawn terrain
        (None, None) => false,
    };

    if needs_terrain_rebuild {
        for e in spawned.terrain.drain(..) {
            commands.entity(e).despawn();
        }
        spawned.terrain_world = world_opt;
        spawned.full_terrain = filters.full_terrain;

        if let Some(w) = world_opt {
            spawn_worldspace_terrain(
                &session,
                &mut commands,
                &mut models,
                &mut spawned,
                w,
                grid_opt,
                filters.full_terrain,
                radius,
            );
        }
    }
    let mut counts = SkipCounts::default();
    // Models are loaded in batches up to the frame budget.
    let frame_budget = if radius < 0 {
        std::time::Duration::from_millis(250)
    } else {
        std::time::Duration::from_millis(100)
    };
    let t = std::time::Instant::now();
    let mut spawned_now = 0usize;
    let trace = std::env::var("NVRE_TRACE").is_ok();
    let refs = if is_exterior && radius != 0 {
        session.exterior_grid_refs(radius).to_vec()
    } else {
        session.cell_refs().to_vec()
    };
    // Resolve missing base objects' model paths in one batch.
    // Cached in `spawned.model_paths` so subsequent frames don't re-resolve them.
    if !spawned.bases_resolved {
        let mut missing_bases: Vec<u32> = refs
            .iter()
            .filter(|r| r.position.is_some())
            .filter_map(|r| r.base.as_deref().and_then(parse_formid))
            .filter(|base| !spawned.model_paths.contains_key(base))
            .collect();
        missing_bases.sort_unstable();
        missing_bases.dedup();

        if !missing_bases.is_empty() {
            let t_paths = std::time::Instant::now();
            let paths = session.model_paths_for(&missing_bases);
            if trace {
                bevy::log::info!(
                    "draw_cell: resolved {} unique base model paths in {:?}",
                    missing_bases.len(),
                    t_paths.elapsed()
                );
            }
            for (base, path) in missing_bases.iter().zip(paths) {
                spawned.model_paths.insert(*base, path);
            }

            // Multithreaded prefetch: kick off background tasks on AsyncComputeTaskPool
            // across all CPU worker threads to decompress archives in parallel.
            let to_prefetch: Vec<String> = spawned
                .model_paths
                .values()
                .flatten()
                .filter(|p| !spawned.loaded_models.contains(&nvre_vfs::normalize(p)))
                .cloned()
                .collect();
            if !to_prefetch.is_empty() {
                let pool = AsyncComputeTaskPool::get();
                let chunk_size =
                    (to_prefetch.len() + pool.thread_num() - 1) / pool.thread_num().max(1);
                for chunk in to_prefetch.chunks(chunk_size) {
                    let chunk = chunk.to_vec();
                    let game_data = models.data.clone();
                    pool.spawn(async move {
                        for p in chunk {
                            let _ = game_data.read(&p);
                        }
                    })
                    .detach();
                }
            }
        }
        spawned.bases_resolved = true;
    }

    // Identify unique models that still need to be loaded into ModelCache.
    // Query directly from unique model_paths with O(1) set check instead of scanning all refs.
    let mut missing_models: Vec<String> = Vec::new();
    let mut seen_models: HashSet<&str> = HashSet::new();
    for path in spawned.model_paths.values().flatten() {
        let key = nvre_vfs::normalize(path);
        if !spawned.loaded_models.contains(&key) && seen_models.insert(path.as_str()) {
            missing_models.push(path.clone());
        }
    }

    // Load models in batches up to the frame budget. Always load at least 1 model per frame.
    let t_models = std::time::Instant::now();
    let mut loaded_any = false;
    for path in missing_models {
        if loaded_any && t_models.elapsed() >= frame_budget {
            break;
        }
        let key = nvre_vfs::normalize(&path);
        let t_model = std::time::Instant::now();
        if models.model(&path).is_some() {
            spawned.loaded_models.insert(key);
            if trace {
                bevy::log::info!("draw_cell: loaded {} in {:?}", path, t_model.elapsed());
            }
        } else {
            // Record failure so it's not repeatedly attempted
            spawned.loaded_models.insert(key);
        }
        loaded_any = true;
    }

    // Spawn all references whose model is now in cache.
    for r in &refs {
        if spawned.refs.contains_key(&r.form_id) {
            continue;
        }
        // A reference that failed once is not retried every frame: the model is
        // missing, and re-resolving it per frame is pure waste. Clearing the
        // spawned list for a cell starts over, which is what opening a new cell
        // does.
        if spawned.failed.contains(&r.form_id) {
            continue;
        }
        let Some(position) = r.position else {
            counts.no_position += 1;
            spawned.failed.insert(r.form_id);
            continue;
        };
        // The base object decides the model.
        let Some(base) = r.base.as_deref() else {
            counts.no_base += 1;
            spawned.failed.insert(r.form_id);
            continue;
        };
        let Some(base_fid) = parse_formid(base) else {
            counts.bad_fid += 1;
            spawned.failed.insert(r.form_id);
            continue;
        };
        let Some(Some(model_path)) = spawned.model_paths.get(&base_fid).cloned() else {
            counts.no_model += 1;
            spawned.failed.insert(r.form_id);
            continue;
        };

        let key = nvre_vfs::normalize(&model_path);
        // Only spawn if the model has already been loaded into ModelCache.
        if !spawned.loaded_models.contains(&key) {
            continue;
        }

        let Some(model) = models.model(&model_path) else {
            counts.no_model += 1;
            spawned.failed.insert(r.form_id);
            continue;
        };

        spawned_now += 1;

        // Composed with the game's own Z-up to Y-up rotation, the way the game's
        // viewer places a reference under its world root. The reference's own
        // rotation goes in too: a door or a road is not axis-aligned in the file,
        // and drawing it as if it were is most of "positioned wrong".
        let rotation = r.rotation.unwrap_or([0.0, 0.0, 0.0]);
        let scale = r.scale.unwrap_or(1.0);
        let tr = game_to_bevy() * ref_transform(position, rotation, scale);
        let root = commands
            .spawn((
                Name::new(
                    r.edid
                        .clone()
                        .unwrap_or_else(|| format!("{:08X}", r.form_id)),
                ),
                RefEntity {
                    form_id: r.form_id,
                    bounds: model.bounds,
                },
                tr,
                Visibility::default(),
            ))
            .id();

        for s in &model.shapes {
            let m = Mat4::from(s.transform);
            commands.entity(root).with_child((
                Mesh3d(s.mesh.clone()),
                MeshMaterial3d(s.material.clone()),
                Transform::from_matrix(m),
                Visibility::default(),
            ));
        }

        spawned.refs.insert(r.form_id, root);
    }
    // How long a frame spends loading models, and how many it managed. A frame
    // that loads a model takes as long as the archive read and mesh build take,
    // which is what makes the viewport stutter while a cell fills in.
    if std::env::var("NVRE_TRACE").is_ok() {
        bevy::log::info!(
            "draw_cell: {} models loaded this frame in {:?} ({} total)",
            spawned_now,
            t.elapsed(),
            spawned.refs.len()
        );
    }
    // A cell that draws nothing has to say why: a reference with no position, no
    // base object, or no model in the load order is skipped, and the count is
    // what distinguishes "nothing to draw" from "drawing failed".
    if spawned.refs.is_empty() && counts.any() {
        bevy::log::warn!("draw_cell: no models spawned; {counts:?}");
    }
}

/// Why references were skipped, for a cell that draws nothing.
#[derive(Debug, Default)]
struct SkipCounts {
    /// No DATA subrecord, so no position to place it at.
    no_position: usize,
    /// No NAME subrecord, so no base object to take a model from.
    no_base: usize,
    /// The base object's FormID did not parse.
    bad_fid: usize,
    /// The base object has no model in the load order.
    no_model: usize,
    /// The asset layer refused the model.
    spawn_failed: usize,
    /// The model spawned but produced no root entity.
    no_root: usize,
}

impl SkipCounts {
    /// Whether any reference was skipped.
    fn any(&self) -> bool {
        self.no_position
            + self.no_base
            + self.bad_fid
            + self.no_model
            + self.spawn_failed
            + self.no_root
            > 0
    }
}

/// Replace every model's material with a flat-coloured standard one.
///
/// A diagnostic, run only when `NVRE_FLAT_MATERIAL` is set. The NIF
/// material is an `ExtendedMaterial` with its own shader, and that shader
/// discards fragments whose alpha is below a cutoff (`nif_material.wgsl`): if
/// the textures fail to load, every fragment is discarded and the viewport is
/// blank with no error anywhere. Drawing the same meshes with a plain material
/// separates "the shader is dropping everything" from "there is nothing in front
/// of the camera".
///
/// Bright magenta on purpose: it is unmistakable against a grey clear colour, so
/// a single visible triangle settles the question.
pub fn flat_material_diagnostic(
    mut commands: Commands,
    mut materials: ResMut<Assets<bevy::pbr::StandardMaterial>>,
    nif_materials: Query<Entity, With<MeshMaterial3d<nvre_assets::NifMaterial>>>,
    mut done: Local<bool>,
) {
    if *done || std::env::var("NVRE_FLAT_MATERIAL").is_err() {
        return;
    }
    *done = true;
    let flat = materials.add(bevy::pbr::StandardMaterial {
        base_color: bevy::prelude::Color::Srgba(bevy::color::Srgba::new(1.0, 0.0, 1.0, 1.0)),
        // Unlit, so a light pointing the wrong way cannot be mistaken for the
        // material being the problem.
        unlit: true,
        ..default()
    });
    let mut swapped = 0usize;
    for e in &nif_materials {
        commands
            .entity(e)
            .remove::<MeshMaterial3d<nvre_assets::NifMaterial>>()
            .insert(MeshMaterial3d(flat.clone()));
        swapped += 1;
    }
    bevy::log::info!("flat_material_diagnostic: swapped {swapped} meshes to flat magenta");
}

/// Click a reference to select it.
#[allow(clippy::too_many_arguments)]
pub fn pick(
    mut session: ResMut<crate::session::Session>,
    gizmo: Res<crate::gizmo::GizmoState>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut focus: ResMut<ViewportFocus>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
    windows: Query<&Window, With<crate::windows::ViewportWindow>>,
    refs: Query<(&RefEntity, &GlobalTransform)>,
    mut clicks: Local<ClickState>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    // If hovering or dragging a gizmo handle, the gizmo consumes the click!
    if gizmo.hovered != crate::gizmo::GizmoHandle::None || gizmo.dragging.is_some() {
        return;
    }
    let Ok((camera, cam_t)) = cameras.single() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_t, cursor) else {
        return;
    };

    // Pick the reference directly under the cursor by testing ray-bounding-box intersection.
    // The closest intersected object along the camera ray is selected.
    let mut best: Option<(f32, u32)> = None;
    for (r, t) in &refs {
        let inv_mat = t.to_matrix().inverse();
        let hit_dist = if let Some((min, max)) = r.bounds {
            ray_local_box_intersection(ray.origin, *ray.direction, inv_mat, min, max)
        } else {
            let to = t.translation() - ray.origin;
            let along = to.dot(*ray.direction);
            if along > 0.0 {
                let perp = (to - *ray.direction * along).length();
                if perp < 60.0 {
                    Some(along)
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(dist) = hit_dist {
            if dist > 0.0 && best.is_none_or(|(d, _)| dist < d) {
                best = Some((dist, r.form_id));
            }
        }
    }
    if let Some((_, fid)) = best {
        session.select(fid);

        // A double-click frames the reference, the way GECK does: the camera moves to
        // look at it. The click has to be on the same reference both times and close
        // enough in time, so an accidental second click does not move the view.
        if clicks.register(fid) {
            frame_reference(&mut focus, &refs, fid);
        }
    } else {
        session.selected = None;
    }
}

fn ray_local_box_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    inv_mat: Mat4,
    min: Vec3,
    max: Vec3,
) -> Option<f32> {
    let local_orig = inv_mat.transform_point3(ray_origin);
    let local_dir = inv_mat.transform_vector3(ray_dir);

    let mut tmin = (min.x - local_orig.x) / local_dir.x;
    let mut tmax = (max.x - local_orig.x) / local_dir.x;
    if tmin > tmax {
        std::mem::swap(&mut tmin, &mut tmax);
    }

    let mut tymin = (min.y - local_orig.y) / local_dir.y;
    let mut tymax = (max.y - local_orig.y) / local_dir.y;
    if tymin > tymax {
        std::mem::swap(&mut tymin, &mut tymax);
    }

    if tmin > tymax || tymin > tmax {
        return None;
    }
    if tymin > tmin {
        tmin = tymin;
    }
    if tymax < tmax {
        tmax = tymax;
    }

    let mut tzmin = (min.z - local_orig.z) / local_dir.z;
    let mut tzmax = (max.z - local_orig.z) / local_dir.z;
    if tzmin > tzmax {
        std::mem::swap(&mut tzmin, &mut tzmax);
    }

    if tmin > tzmax || tzmin > tmax {
        return None;
    }
    if tzmin > tmin {
        tmin = tzmin;
    }
    if tzmax < tmax {
        tmax = tzmax;
    }

    let t = if tmin >= 0.0 { tmin } else { tmax };
    if t < 0.0 {
        return None;
    }

    let hit_local = local_orig + local_dir * t;
    let hit_world = inv_mat.inverse().transform_point3(hit_local);
    Some((hit_world - ray_origin).dot(ray_dir))
}

/// Point the camera at a reference.
///
/// Shared by the viewport's own double-click, the `F` hotkey and the references
/// panel's, so framing an object is the same however it was asked for.
fn frame_reference(
    focus: &mut ViewportFocus,
    refs: &Query<(&RefEntity, &GlobalTransform)>,
    form_id: u32,
) {
    // Look at the model itself, not the cursor's ray hit, so the framing is exact
    // even when the ray only passed near the reference.
    let Some((_, t)) = refs.iter().find(|(r, _)| r.form_id == form_id) else {
        return;
    };
    // The same distance the editor opens a cell at, so framing one reference and
    // framing the cell it is in agree. The heading is left alone and the view is
    // levelled off: the camera swings round to the object rather than snapping to
    // a fixed angle.
    request_view(
        focus,
        ViewPose {
            target: t.translation(),
            distance: ViewportFocus::DEFAULT_DISTANCE,
            pitch: Some(0.0),
            yaw: None,
        },
    );
}

/// Frame whatever the references panel asked for.
///
/// The panel draws in its own window's schedule and the camera lives here, so the
/// intent arrives as a message rather than a call.
pub fn apply_frame_requests(
    mut focus: ResMut<ViewportFocus>,
    refs: Query<(&RefEntity, &GlobalTransform)>,
    mut requests: MessageReader<crate::FrameRequested>,
) {
    for r in requests.read() {
        frame_reference(&mut focus, &refs, r.form_id);
    }
}

/// What the last click was, to tell a double-click from two singles.
///
/// Public because a `Local<T>` system parameter makes its type part of the
/// system's signature, which the app that registers the system has to name.
#[derive(Default)]
pub struct ClickState {
    /// The reference the last click selected.
    last: Option<u32>,
    /// When the last click happened.
    at: Option<std::time::Instant>,
    /// Whether the pair the last click completed has already been reported.
    matched: bool,
}

impl ClickState {
    /// Whether this click is the second of a double-click on the same reference.
    ///
    /// Two clicks within `GAP` on the same reference count as one double-click.
    /// A matched pair is consumed, so three clicks in a row read as a single
    /// double-click followed by a fresh click rather than two double-clicks.
    pub fn register(&mut self, fid: u32) -> bool {
        const GAP: std::time::Duration = std::time::Duration::from_millis(400);
        let now = std::time::Instant::now();
        let in_time = self.at.is_some_and(|t| now.duration_since(t) < GAP);
        let double = self.last == Some(fid) && in_time && !self.matched;
        // Start a new pair either way: the click that completes one is the first
        // of the next, unless the pair was just matched.
        self.matched = double;
        self.last = Some(fid);
        self.at = Some(now);
        double
    }
}

/// The decoder's FormID text form.
fn parse_formid(s: &str) -> Option<u32> {
    u32::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

/// The centre of a cell's references, in Bevy space.
///
/// Where the camera should look when a cell opens. The mapping is the same one
/// `ref_transform` applies, so a camera target here lines up with the models
/// drawn from those positions. A cell's references can be spread widely, so this
/// is their mean, which puts the camera inside the cell rather than at a corner.
/// The centre of a cell's references, in the space the models are in (Bevy space).
fn cell_centre(refs: &[crate::session::RefRow]) -> Option<Vec3> {
    let mut sum = Vec3::ZERO;
    let mut n = 0.0;
    for r in refs {
        let Some(p) = r.position else { continue };
        sum += (game_to_bevy() * ref_transform(p, [0.0; 3], 1.0)).translation;
        n += 1.0;
    }
    (n > 0.0).then(|| sum / n)
}

/// Spawn landscape terrain for the entire worldspace (or wide horizon) around the active cell.
#[allow(clippy::too_many_arguments)]
fn spawn_worldspace_terrain(
    session: &crate::session::Session,
    commands: &mut Commands,
    models: &mut ModelCtx,
    spawned: &mut Spawned,
    world_id: u32,
    centre_grid: Option<(i32, i32)>,
    full_terrain: bool,
    local_radius: i32,
) {
    let Ok(lo) = session.load_order() else {
        return;
    };
    let Some(ws) = session.index.worldspaces.get(&world_id).cloned() else {
        return;
    };

    let mut mat_cache: HashMap<String, Handle<nvre_assets::nif_material::NifMaterial>> =
        HashMap::new();

    let cells_to_load: Vec<((i32, i32), u32)> = if full_terrain || local_radius < 0 {
        // Load the landscape for the whole worldspace.
        ws.grid
            .iter()
            .filter_map(|(&coord, &cid)| {
                let land_id = session.index.cells.get(&cid)?.land?;
                Some((coord, land_id))
            })
            .collect()
    } else {
        let (cx, cy) = centre_grid.unwrap_or((0, 0));
        let mut list = Vec::new();
        for dx in -local_radius..=local_radius {
            for dy in -local_radius..=local_radius {
                let coord = (cx + dx, cy + dy);
                if let Some(&cid) = ws.grid.get(&coord) {
                    if let Some(c) = session.index.cells.get(&cid) {
                        if let Some(land_id) = c.land {
                            list.push((coord, land_id));
                        }
                    }
                }
            }
        }
        list
    };

    for (coord, land_id) in cells_to_load {
        spawn_single_cell_terrain(
            lo,
            &session.schema,
            commands,
            models,
            spawned,
            &mut mat_cache,
            land_id,
            coord,
        );
    }
}

/// Spawn the 4 terrain quadrant meshes for a single exterior cell using a shared material cache.
#[allow(clippy::too_many_arguments)]
fn spawn_single_cell_terrain(
    lo: &nvre_esm::LoadOrder,
    schema: &nvre_esm::schema::Schema,
    commands: &mut Commands,
    models: &mut ModelCtx,
    spawned: &mut Spawned,
    mat_cache: &mut HashMap<String, Handle<nvre_assets::nif_material::NifMaterial>>,
    land_id: u32,
    grid: (i32, i32),
) {
    let Some(land) = load_land(lo, schema, land_id, LandParts::ALL) else {
        return;
    };
    let Some(heights) = land.heights.as_ref() else {
        return;
    };

    let cell_origin = Vec2::new(
        grid.0 as f32 * nvre_world::CELL_SIZE,
        grid.1 as f32 * nvre_world::CELL_SIZE,
    );

    for q in 0..4u8 {
        let base_layer = land.layers.iter().rev().find(|l| l.base && l.quadrant == q);
        let tex_path = base_layer
            .and_then(|l| l.texture)
            .and_then(|t| land_texture(lo, schema, t))
            .and_then(|t| t.diffuse)
            .unwrap_or_else(|| DEFAULT_DIFFUSE.to_string());

        let tex_rel = tex_path.replace('/', "\\").to_ascii_lowercase();
        let tex_file = if tex_rel.starts_with("textures\\") {
            tex_rel
        } else {
            format!("textures\\{tex_rel}")
        };

        let mat_handle = match mat_cache.get(&tex_file) {
            Some(h) => h.clone(),
            None => {
                let img_handle = models.textures.get(
                    &tex_file,
                    true,
                    &models.data,
                    &mut models.images,
                    models.formats.as_deref(),
                );
                let mat = StandardMaterial {
                    base_color: if img_handle.is_some() {
                        Color::WHITE
                    } else {
                        Color::srgb(0.55, 0.50, 0.42)
                    },
                    base_color_texture: img_handle,
                    perceptual_roughness: 0.95,
                    reflectance: 0.2,
                    cull_mode: None,
                    ..default()
                };
                let h = models.materials.add(nvre_assets::nif_material(mat));
                mat_cache.insert(tex_file, h.clone());
                h
            }
        };

        if let Some(mesh) = terrain_quadrant_mesh(&land, heights, q, cell_origin) {
            let mesh_handle = models.meshes.add(mesh);
            let e = commands
                .spawn((
                    Name::new(format!("Terrain Q{q}")),
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(mat_handle),
                    Transform::IDENTITY,
                    Visibility::default(),
                ))
                .id();
            spawned.terrain.push(e);
        }
    }
}

/// Generate a Bevy mesh for one 17x17 terrain quadrant.
fn terrain_quadrant_mesh(land: &Land, heights: &[f32], q: u8, cell_origin: Vec2) -> Option<Mesh> {
    let qx = (q % 2) as usize * (QUAD_SIDE - 1);
    let qy = (q / 2) as usize * (QUAD_SIDE - 1);
    let n = QUAD_SIDE * QUAD_SIDE;

    let mut pos = Vec::with_capacity(n);
    let mut nor = Vec::with_capacity(n);
    let mut uv = Vec::with_capacity(n);
    let mut tan = Vec::with_capacity(n);

    let g2b = game_to_bevy();
    let tile = 256.0_f32;

    for r in 0..QUAD_SIDE {
        for c in 0..QUAD_SIDE {
            let (gr, gc) = (qy + r, qx + c);
            let i = gr * SIDE + gc;
            let local_x = gc as f32 * SPACING;
            let local_y = gr as f32 * SPACING;
            let world_x = cell_origin.x + local_x;
            let world_y = cell_origin.y + local_y;
            let world_z = heights[i];

            let p_game = Vec3::new(world_x, world_y, world_z);
            let p_bevy = g2b.transform_point(p_game);
            pos.push(p_bevy.to_array());

            let normal_game = land
                .normals
                .as_ref()
                .map(|v| {
                    Vec3::new(v[i][0] as f32, v[i][1] as f32, v[i][2] as f32).normalize_or(Vec3::Z)
                })
                .unwrap_or(Vec3::Z);
            let normal_bevy = g2b.rotation * normal_game;
            nor.push(normal_bevy.to_array());

            let east_game = (Vec3::X - normal_game * normal_game.x).normalize_or(Vec3::X);
            let east_bevy = g2b.rotation * east_game;
            tan.push([east_bevy.x, east_bevy.y, east_bevy.z, -1.0]);

            uv.push([world_x / tile, world_y / tile]);
        }
    }

    let mut idx = Vec::with_capacity((QUAD_SIDE - 1) * (QUAD_SIDE - 1) * 6);
    for r in 0..QUAD_SIDE - 1 {
        for c in 0..QUAD_SIDE - 1 {
            let i = (r * QUAD_SIDE + c) as u32;
            let (e, nn, ne) = (i + 1, i + QUAD_SIDE as u32, i + QUAD_SIDE as u32 + 1);
            idx.extend_from_slice(&[i, e, ne, i, ne, nn]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tan);

    if let Some(colors) = &land.colors {
        let mut col = Vec::with_capacity(n);
        for r in 0..QUAD_SIDE {
            for c in 0..QUAD_SIDE {
                let i = (qy + r) * SIDE + (qx + c);
                let rgb = colors[i];
                col.push([
                    rgb[0] as f32 / 255.0,
                    rgb[1] as f32 / 255.0,
                    rgb[2] as f32 / 255.0,
                    1.0,
                ]);
            }
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    }

    mesh.insert_indices(Indices::U32(idx));
    Some(mesh)
}
