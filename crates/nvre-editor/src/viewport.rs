//! The viewport: a 3D view of the open cell's placed references, with a camera
//! and click-to-select.
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
//! The viewport's camera is a **free** camera, not an orbit. There is no pivot and
//! no anchor: every motion is the camera's own translation or rotation, so a drag
//! turns the view in place and the wheel walks it along the line it is looking
//! down — the way GECK's viewer moves, and the way a person walks up to a wall.
//!
//! The editor owns this outright. Bevy ships a `FreeCamera`, but it is a *fly*
//! camera: it reads Bevy's own accumulated-mouse-motion resource, drives WASD, and
//! grabs the cursor. None of that is what a viewport wants — the viewport's input
//! is the same `MouseMotion` stream the gizmo and the picking read, and a second
//! reader of it is how one drag stops being one drag. So the motion is implemented
//! here, directly on the `Transform`, and [`ViewportCameraPlugin`] is the whole of
//! it.
//!
//! The pose is the `Transform`, with no state alongside it: a drag changes the
//! heading or the position, a frame request rewrites both. [`ViewportFocus`] holds
//! the intent behind a request — where to look and how far away — and the pending
//! pose waiting to be applied.

use crate::session::LightData;
use bevy::asset::RenderAssetUsages;
use bevy::ecs::message::MessageReader;
use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use nvre_assets::model::ModelCtx;
use nvre_world::land::{
    land_texture, load_land, Land, LandParts, DEFAULT_DIFFUSE, QUAD_SIDE, SIDE, SPACING,
};
use nvre_world::transform::ref_rotation;
use std::collections::{HashMap, HashSet};

/// Marks the viewport's free camera.
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

/// What the viewport wants the camera to do, as the editor thinks of it.
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
    /// The lights the cell's references emitted, by their reference's FormID.
    ///
    /// Separate from `refs` because a light is a real Bevy light rather than a
    /// mesh: it has no model to load and no bounds to select with, and it is
    /// despawned with the cell like everything else.
    pub lights: HashMap<u32, Entity>,
    /// Base FormID → `LIGH` data, resolved once per base object.
    pub light_paths: HashMap<u32, Option<LightData>>,
    /// The cell and radius the lights were spawned for.
    pub light_cell: Option<u32>,
    pub light_radius: i32,
}

/// Spawn the viewport's ambient light.
///
/// The camera itself is spawned by [`crate::windows::spawn_viewport`], which is
/// what ties it to the viewport window. Spawning a second camera here would
/// leave one with no window to render to, and its egui context's output would
/// never be consumed.
///
/// The ambient is the viewport's only light: it is on when the editor opens, and
/// the hotbar's Ambient toggle turns it off — see
/// [`crate::windows::ViewportHotbar`]. So this value is the default the toggle
/// starts from, not a setting fixed for the session.
pub fn setup(mut commands: Commands) {
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.7, 0.75, 0.8),
        brightness: 800.0,
        ..default()
    });
}

/// Keep the camera where the focus says it is.
///
/// The camera owns the pose between moves, so this only acts when the editor has
/// actually asked for a view: [`ViewportFocus`] holds a pending [`ViewPose`], and
/// it is cleared here once the camera has been put there. Reading the pose back
/// afterwards is what keeps the debug window honest — it describes where the
/// camera is, not the last thing that was requested.
pub fn sync_focus(
    mut cameras: Query<&mut Transform, With<ViewportCamera>>,
    mut focus: ResMut<ViewportFocus>,
) {
    let Ok(mut transform) = cameras.single_mut() else {
        return;
    };
    if let Some(pose) = focus.pending.take() {
        place_camera(&mut transform, pose);
    }
    // The focus follows the camera: a free camera has no pivot to keep it aimed
    // at anything, so what it is looking at is its own position pushed out along
    // its own heading. The distance is that same length, which is what the debug
    // window and the framing requests both want.
    let forward = *transform.forward();
    focus.target = transform.translation + forward * focus.distance;
}

/// Put the camera at a pose.
///
/// The heading is kept unless the pose names one — only the distance and the aim
/// change — so a view that is already turned towards an object swings round to it
/// rather than snapping to a fixed angle.
pub fn place_camera(transform: &mut Transform, pose: ViewPose) {
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
/// The whole of the editor's camera handling is here. Right-drag turns the view,
/// middle-drag slides it, and the wheel walks it forward or back along the line it
/// is looking down. None of them has a pivot: a turn rotates the camera in place,
/// and a slide and a walk translate it.
pub struct ViewportCameraPlugin;

impl Plugin for ViewportCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraDrag>()
            .init_resource::<ViewportFocus>()
            .add_systems(
                PreUpdate,
                (start_camera_motions, feed_camera_input, end_camera_motions).chain(),
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
    /// Right-drag: turn the view in place.
    Turn,
    /// Middle-drag: slide the view across the world.
    Slide,
}

/// The button that starts each motion.
///
/// Right-drag turns and middle-drag slides. This is the mapping Bevy's own CAD
/// example uses for its two drags, and the reason for it: a turn is the motion a
/// hand reaches for constantly and the right button is the one under it, while a
/// slide is deliberate and gets the middle button, which nothing else wants.
///
/// Public because it is the whole of the editor's own camera policy; everything
/// else about the camera is the motion itself. It is a pure function of the input
/// state so the mapping can be pinned without a window.
pub fn motion_for(buttons: &ButtonInput<MouseButton>) -> Option<Motion> {
    if buttons.just_pressed(MouseButton::Right) {
        return Some(Motion::Turn);
    }
    if buttons.just_pressed(MouseButton::Middle) {
        return Some(Motion::Slide);
    }
    None
}

/// Whether the motion a button started is still held.
///
/// One button each, so a motion ends when its own button comes up and nothing
/// else can end it early.
pub fn motion_held(motion: Motion, buttons: &ButtonInput<MouseButton>) -> bool {
    match motion {
        Motion::Turn => buttons.pressed(MouseButton::Right),
        Motion::Slide => buttons.pressed(MouseButton::Middle),
    }
}

/// How far one wheel notch moves the camera, in game units.
///
/// A fixed distance rather than a fraction of the camera's distance. Bevy's zoom
/// is proportional to the depth it is zooming along, which is what a *zoom* is:
/// at 16,000 units out — a worldspace at its viewing distance — a notch covers
/// 160 times the ground one does at 1,000, and three notches cross the whole cell
/// and end up inside the terrain. A dolly wants the same step everywhere.
///
/// A cell is 4,096 units across, so one notch is a sixteenth of that: a small
/// step, the sort you make to line the camera up with a door or a wall rather than
/// the sort you make to cross a cell. Scrolling across a whole worldspace takes a
/// deliberate run of notches, which is what keeps the wheel from overshooting.
const SCROLL_STEP: f32 = 256.0;

/// How much less a wheel notch moves the camera.
///
/// The wheel's own units are fine-grained — a line wheel reports whole notches and
/// a trackpad reports fractions — so the step is divided here to make a notch a
/// small move rather than a leap. A cell is 4,096 units across; at 16,000 units a
/// worldspace is a few notches away, and a notch that covered a sixteenth of a cell
/// crossed it in one.
const SCROLL_DIVISOR: f32 = 2.0;

/// How far a drag of one screen pixel turns the view, in radians.
///
/// A full sweep across the viewport is a turn of a few degrees, which is what a
/// viewport's drag feels like: the view follows the hand closely without being so
/// sensitive that a pixel of jitter is visible.
const DRAG_TURN: f32 = 0.006;

/// How far the flight keys move the camera in one second, in game units.
///
/// A cell is 4,096 units across, so this is about a third of a cell a second: fast
/// enough to cross a worldspace by holding a key, slow enough to stop on a
/// reference. A game unit is ~4.19 cm, so this is a walk and not a sprint — which
/// is what a viewport wants, since the alternative is the wheel.
const FLIGHT_SPEED: f32 = 1400.0;

/// Which flight keys move the camera along which direction, while a drag is held.
///
/// `W` and `S` walk along the view, `A` and `D` strafe across it. Q and E are not
/// bound: the view is level-locked, so there is no up or down to fly along, and
/// giving them a meaning would be a third motion nobody asked for.
const FLIGHT_KEYS: [(KeyCode, FlightDir); 4] = [
    (KeyCode::KeyW, FlightDir::Forward),
    (KeyCode::KeyS, FlightDir::Backward),
    (KeyCode::KeyA, FlightDir::Left),
    (KeyCode::KeyD, FlightDir::Right),
];

/// Which way a flight key moves the camera.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FlightDir {
    Forward,
    Backward,
    Left,
    Right,
}

/// Whether a drag is in progress that flight keys should be read for.
///
/// Only a right-drag arms them. A middle-drag is a deliberate slide, and adding
/// flight keys to it would make the two motions fight; and with nothing held, the
/// keys are free for whatever else wants them.
fn flight_armed(buttons: &ButtonInput<MouseButton>) -> bool {
    buttons.pressed(MouseButton::Right)
}

/// Which motion is in progress.
///
/// A resource rather than a `Local`, because the input, motion and end systems are
/// separate and all need it.
#[derive(Resource, Default)]
pub struct CameraDrag {
    motion: Option<Motion>,
}

/// Start a motion on a button press.
fn start_camera_motions(mut drag: ResMut<CameraDrag>, buttons: Res<ButtonInput<MouseButton>>) {
    if let Some(motion) = motion_for(&buttons) {
        drag.motion = Some(motion);
    }
}

/// Feed the frame's movement, wheel and flight keys to the camera.
#[allow(clippy::too_many_arguments)]
fn feed_camera_input(
    drag: Res<CameraDrag>,
    focus: Res<ViewportFocus>,
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cameras: Query<(&mut Transform, &Projection), With<ViewportCamera>>,
    windows: Query<&Window, With<crate::windows::ViewportWindow>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
) {
    let Ok((mut transform, projection)) = cameras.single_mut() else {
        return;
    };
    // The viewport's height, which is what a pixel's size is measured against. A
    // slide is a screen-space motion, so it is expressed in pixels and converted
    // at the camera's own depth.
    let viewport_height = windows
        .single()
        .map(|w| w.resolution.height())
        .unwrap_or(800.0)
        .max(1.0);

    // The wheel, handled whether or not a drag is in progress: walking is the one
    // motion that does not need a button held, and a drag that is already turning
    // or sliding should still be able to walk.
    //
    // A notch is a fixed distance along the camera's own forward vector, so the
    // same notch is worth the same ground at any distance. The wheel is not a zoom:
    // it does not scale with how far away the camera is.
    let mut walked = 0.0;
    for w in wheel.read() {
        let step = match w.unit {
            MouseScrollUnit::Line => SCROLL_STEP,
            MouseScrollUnit::Pixel => SCROLL_STEP / 150.0,
        };
        walked += w.y * step / SCROLL_DIVISOR;
    }
    if walked != 0.0 {
        let forward = *transform.forward();
        transform.translation += forward * walked;
    }

    // The flight keys, read while a right-drag is held. They are the walking
    // counterpart to the wheel: the wheel steps, the keys move continuously for as
    // long as they are down, which is what makes crossing a worldspace a held key
    // rather than a run of notches.
    //
    // Armed only by a right-drag, so the keys stay free when nothing is held and
    // do not fight a middle-drag slide.
    if flight_armed(&buttons) {
        let mut travel = Vec3::ZERO;
        for (key, dir) in FLIGHT_KEYS {
            if keys.pressed(key) {
                let axis = match dir {
                    FlightDir::Forward => *transform.forward(),
                    FlightDir::Backward => -*transform.forward(),
                    FlightDir::Left => -*transform.right(),
                    FlightDir::Right => *transform.right(),
                };
                travel += axis;
            }
        }
        if travel != Vec3::ZERO {
            transform.translation += travel.normalize() * FLIGHT_SPEED * time.delta_secs();
        }
    }

    let Some(current) = drag.motion else {
        // Read and drop, so a drag from before this motion does not arrive as one
        // huge delta the next time a button is pressed.
        motion.clear();
        return;
    };
    // `MouseMotion`'s delta is cumulative since the last read, so the frame's
    // movement is the sum of the messages read this frame.
    let screenspace: Vec2 = motion.read().map(|m| m.delta).sum();
    if screenspace == Vec2::ZERO {
        return;
    }

    match current {
        Motion::Turn => {
            // A turn is a rotation in place: the eye does not move, the heading
            // does. Yaw is about the world up axis so the view stays level, and
            // pitch about the camera's own right axis so it can look straight up
            // and down. Neither axis moves the camera, which is what makes this a
            // turn and not a swing.
            transform.rotate_y(-screenspace.x * DRAG_TURN);
            transform.rotate_local_x(-screenspace.y * DRAG_TURN);
        }
        Motion::Slide => {
            // A slide is a translation in the camera's own plane. The pixels are
            // converted at the camera's own distance, which is what keeps the thing
            // under the cursor under the cursor: the further the camera is, the
            // more ground a pixel covers, so the same drag moves the same screen
            // fraction at any distance.
            let Some(pixels) =
                slide_pixels(projection, screenspace, focus.distance, viewport_height)
            else {
                return;
            };
            let right = *transform.right();
            let up = *transform.up();
            transform.translation += right * -pixels.x + up * pixels.y;
        }
    }
}

/// The world distance a slide of `screenspace` pixels covers.
///
/// A pixel's size at unit depth from the projection, scaled by the camera's own
/// distance from what it is looking at. At unit depth a pixel is
/// `2 * tan(fov/2) / viewport_height`, and the camera is `distance` away, so a
/// pixel there is that times the distance.
fn slide_pixels(
    projection: &Projection,
    screenspace: Vec2,
    distance: f32,
    viewport_height: f32,
) -> Option<Vec2> {
    let fov = match projection {
        Projection::Perspective(p) => p.fov,
        _ => return None,
    };
    let units_per_pixel = 2.0 * (fov * 0.5).tan() / viewport_height;
    Some(screenspace * units_per_pixel * distance)
}

/// End the motion when the button that started it comes up.
fn end_camera_motions(mut drag: ResMut<CameraDrag>, buttons: Res<ButtonInput<MouseButton>>) {
    let Some(motion) = drag.motion else {
        return;
    };
    if motion_held(motion, &buttons) {
        return;
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

/// Spawn the lights the cell's references emit.
///
/// Every placed reference whose base object is a `LIGH` gets a real Bevy light at
/// its position: a `PointLight`, or a `SpotLight` when the record's spot flag is
/// set. These are the game's own lights, read from the load order — not the
/// viewport's ambient, which is a flat global term and a separate switch
/// entirely (see [`crate::windows::ViewportHotbar`]).
///
/// A light is not a model: it has no mesh, no bounds and nothing to click, so
/// it is spawned from the references directly rather than through the model
/// loader. A light whose base object is missing from the load order is skipped,
/// exactly as a model would be.
pub fn draw_cell_lights(
    mut commands: Commands,
    mut session: ResMut<crate::session::Session>,
    mut spawned: ResMut<Spawned>,
) {
    let Some(cell) = session.cell else {
        return;
    };
    // The same references the models come from, so a light is drawn for exactly
    // the cells a model is.
    let (refs, radius) = if spawned.radius != 0 {
        (
            session.exterior_grid_refs(spawned.radius).to_vec(),
            spawned.radius,
        )
    } else {
        (session.cell_refs().to_vec(), 0)
    };
    // A different cell: drop the lights with the models.
    if spawned.light_cell != Some(cell) || spawned.light_radius != radius {
        for (_, e) in spawned.lights.drain() {
            commands.entity(e).despawn();
        }
        spawned.light_cell = Some(cell);
        spawned.light_radius = radius;
    }
    // Resolve the base objects' light data in one batch, as the model paths are.
    let bases: Vec<u32> = refs
        .iter()
        .filter(|r| r.position.is_some())
        .filter_map(|r| r.base.as_deref().and_then(parse_formid))
        .collect();
    let mut missing: Vec<u32> = bases
        .iter()
        .copied()
        .filter(|b| !spawned.light_paths.contains_key(b))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    if !missing.is_empty() {
        let found = session.light_paths_for(&missing);
        for (base, light) in missing.iter().zip(found) {
            spawned.light_paths.insert(*base, light);
        }
    }
    for r in &refs {
        if spawned.lights.contains_key(&r.form_id) {
            continue;
        }
        let Some(position) = r.position else { continue };
        let Some(base) = r.base.as_deref().and_then(parse_formid) else {
            continue;
        };
        let Some(Some(data)) = spawned.light_paths.get(&base).cloned() else {
            continue;
        };
        // `Off By Default` means the light is off until something turns it on,
        // which in the game is a script or a linked reference. The editor has no
        // such state, so the flag is honoured literally: the light is not drawn.
        if data.off_by_default {
            continue;
        }
        let transform =
            game_to_bevy() * ref_transform(position, r.rotation.unwrap_or([0.0, 0.0, 0.0]), 1.0);
        let entity = if data.spot {
            commands
                .spawn((
                    Name::new(format!("light {:08X}", r.form_id)),
                    light_bundle_spot(&data),
                    transform,
                    Visibility::default(),
                ))
                .id()
        } else {
            commands
                .spawn((
                    Name::new(format!("light {:08X}", r.form_id)),
                    light_bundle(&data),
                    transform,
                    Visibility::default(),
                ))
                .id()
        };
        spawned.lights.insert(r.form_id, entity);
    }
    if std::env::var("NVRE_TRACE").is_ok() {
        bevy::log::info!(
            "draw_cell_lights: cell {:08X} radius {} -> {} lights of {} refs",
            cell,
            radius,
            spawned.lights.len(),
            refs.len()
        );
    }
}

/// A point light from a `LIGH`'s data.
///
/// The mapping is the game's own, not a guess: the radius is the light's reach,
/// the colour is its colour, and the intensity is the fade value scaled by a
/// constant that puts a full-fade light at a readable brightness.
fn light_bundle(data: &LightData) -> PointLight {
    PointLight {
        color: data.bevy_color(),
        intensity: data.fade * LIGHT_LUMENS,
        range: data.radius,
        radius: 0.0,
        shadow_maps_enabled: false,
        ..default()
    }
}

/// A spotlight from a `LIGH`'s data.
///
/// The game's `FOV` is the cone's full angle in degrees; Bevy's `outer_angle` is
/// a half-angle in radians, so it is halved and converted. The inner angle is
/// left at zero, which makes the cone's edge hard — the game's own soft edge is
/// a falloff the viewport's shader does not reproduce.
fn light_bundle_spot(data: &LightData) -> SpotLight {
    SpotLight {
        color: data.bevy_color(),
        intensity: data.fade * LIGHT_LUMENS,
        range: data.radius,
        radius: 0.0,
        outer_angle: (data.fov.to_radians() / 2.0).clamp(0.0, std::f32::consts::FRAC_PI_2),
        shadow_maps_enabled: false,
        ..default()
    }
}

/// The intensity of a light whose fade is 1.0, in lumens.
///
/// **This is the number that makes the lights visible at all**, and it is not a
/// guess — it is the ambient's scale, worked backwards.
///
/// The viewport's ambient is `GlobalAmbientLight { colour 0.7, brightness 800 }`,
/// which Bevy turns into `ambient_color = 0.7 × 800 = 560` in linear units, or
/// ~358 after the sRGB decode. Every surface gets that, flat, with no falloff
/// and no direction. A lamp has to produce a comparable term or it is invisible
/// against it.
///
/// Bevy's point light contributes `intensity / (4π) × attenuation(d)` where
/// `attenuation` is `1/d²` smoothed to zero at the light's `range`. So the
/// intensity needed for a lamp to match the ambient at a given distance is
/// `4π × ambient / attenuation(d)`:
///
/// | ambient | lamp range | match at | lumens needed |
/// |---|---|---|---|
/// | 358 | 512 | d = 256 | ~3.4 × 10⁸ |
/// | 358 | 512 | d = 50 | ~1.1 × 10⁷ |
///
/// A lamp's own radius is its useful range — a surface inside it should be
/// lit, one outside it should not — so the constant is set for the **near**
/// case, where the falloff is gentle and the light reads as a lamp rather than
/// as a point. `10⁸` is the value that makes a fade-1.0 light at the middle of
/// its own radius comparable to the ambient around it.
///
/// The first attempt at this was `4000`, which is Bevy's own scale for a
/// *close* light in a *dark* scene. Here the ambient is 358 and the lamp's
/// radius is hundreds of units, so a lamp at d=50 contributed 0.127 — 2800×
/// less than the ambient, and invisible. The lights were spawning correctly the
/// whole time; they were simply too dim to see.
const LIGHT_LUMENS: f32 = 20_000.0;

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
