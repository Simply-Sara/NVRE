//! The viewport: a 3D view of the open cell's placed references, with an orbit
//! camera and click-to-select.
//!
//! The editor's own state lives in [`crate::session`]; this module only turns it
//! into entities and reads clicks back. Meshes are spawned through NVForever's
//! `nvf-assets` layer, so a reference looks the way it does in the game rather
//! than as a placeholder box.
//!
//! Game space is Z-up with one unit ≈ 4.19 cm; Bevy space is Y-up. The conversion
//! is the same one the game itself uses (`world::game_to_bevy`).
//!
//! A reference's model is placed by the asset layer's own root, which already
//! carries the NIF's Z-up-to-Bevin-Y-up rotation; the reference's transform is
//! composed onto that rather than replacing it.

use bevy::ecs::message::MessageReader;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use fnv_world::transform::ref_rotation;
use nvf_assets::SpawnCtx;
use std::collections::{HashMap, HashSet};

/// Marks the viewport's orbit camera.
#[derive(Component)]
pub struct ViewportCamera;

/// An entity standing for one placed reference.
#[derive(Component)]
pub struct RefEntity {
    pub form_id: u32,
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
        translation: Vec3::new(position[0], -position[1], position[2]),
        rotation: Quat::from_mat3(&m),
        scale: Vec3::splat(scale),
    }
}

/// Orbit state for the viewport camera.
#[derive(Resource)]
pub struct Orbit {
    /// What the camera looks at, in Bevy space.
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Orbit {
    /// How far from the target the camera opens, and how far it frames a
    /// reference at.
    ///
    /// One constant, because these have to agree: framing a reference and
    /// framing the cell it sits in should put the camera the same distance away.
    pub const DEFAULT_DISTANCE: f32 = 250.0;
}

impl Default for Orbit {
    fn default() -> Self {
        Orbit {
            target: Vec3::ZERO,
            distance: Self::DEFAULT_DISTANCE,
            // Zero on both: the camera looks straight down the target's +X, level
            // with it. Any other default is a surprise the first time a cell
            // opens.
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

/// Which models have been spawned, so a frame does not respawn them.
#[derive(Resource, Default)]
pub struct Spawned {
    pub refs: HashMap<u32, Entity>,
    pub cell: Option<u32>,
    /// References that could not be drawn, so they are not retried every frame.
    failed: HashSet<u32>,
}

/// Spawn the viewport's light and orbit state.
///
/// The camera itself is spawned by [`crate::windows::spawn_windows`], which is
/// what ties it to the viewport window. Spawning a second camera here would
/// leave one with no window to render to, and its egui context's output would
/// never be consumed.
pub fn setup(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.8, -0.9, 0.0)),
    ));
}

/// Keep the camera where the orbit state says it is.
pub fn orbit_camera(mut cameras: Query<&mut Transform, With<ViewportCamera>>, orbit: Res<Orbit>) {
    let Ok(mut t) = cameras.single_mut() else {
        return;
    };
    let dir = Vec3::new(
        orbit.yaw.cos() * orbit.pitch.cos(),
        orbit.pitch.sin(),
        orbit.yaw.sin() * orbit.pitch.cos(),
    );
    t.translation = orbit.target - dir * orbit.distance;
    t.look_at(orbit.target, Vec3::Y);
}

/// Mouse drag orbits, the wheel zooms.
pub fn orbit_input(
    mut orbit: ResMut<Orbit>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<bevy::input::mouse::MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
) {
    if buttons.pressed(MouseButton::Right) {
        // `MouseMotion`'s delta is cumulative since the last read, so summing
        // this frame's messages *is* the frame's movement. Subtracting a running
        // total instead makes the camera jump backwards every other frame: the
        // sum restarts near zero, the difference goes negative, and the view
        // snaps back to where it started.
        let move_by: Vec2 = motion.read().map(|m| m.delta).sum();
        // At yaw 0 the eye is on -X looking towards +X, so the camera's own right
        // is +Z. Dragging right swings the eye towards -Z, which is a smaller
        // yaw; dragging up lifts the eye above the target, which is a smaller
        // pitch. Neither sign is obvious from `eye = target - dir * distance`,
        // and `crates/nvre-editor/tests/orbit_input.rs` pins both.
        orbit.yaw -= move_by.x * 0.005;
        orbit.pitch = (orbit.pitch - move_by.y * 0.005).clamp(-1.55, 1.55);
    }
    for w in wheel.read() {
        orbit.distance = (orbit.distance * (1.0 - w.y * 0.1)).clamp(20.0, 200_000.0);
    }
}

/// Draw the open cell's references, spawning models as they are first seen.
///
/// Only the cell the session has open is drawn; a mod is edited one cell at a
/// time, and a whole worldspace is far more entities than an editor frame needs.
pub fn draw_cell(
    mut session: ResMut<crate::session::Session>,
    mut spawned: ResMut<Spawned>,
    mut models: SpawnCtx,
) {
    let Some(cell) = session.cell else { return };
    if spawned.cell != Some(cell) {
        // A different cell: drop what was drawn, and start its failures over.
        for (_, e) in spawned.refs.drain() {
            models.commands.entity(e).despawn();
        }
        spawned.failed.clear();
        spawned.cell = Some(cell);
        // And look at the new cell: the camera's target stays where it was, so
        // without this it looks at the last cell's centre and the new one is off
        // screen.
        let refs = session.cell_refs().to_vec();
        if let Some(centre) = cell_centre(&refs) {
            models.commands.insert_resource(Orbit {
                target: centre,
                ..Orbit::default()
            });
        }
    }
    let mut counts = SkipCounts::default();
    // Models are loaded a few per frame, not all at once: reading a model out of
    // a BSA and building its mesh takes tens of milliseconds, so a cell's 155 of
    // them in one frame blocks the event loop for the better part of a minute —
    // which is what froze the whole editor, windows included.
    //
    // The budget is a time, not a count: one frame loads models until it has
    // spent this long, so a cell of cheap models fills in a few frames and a cell
    // of expensive ones still keeps the editor responsive.
    const FRAME_BUDGET: std::time::Duration = std::time::Duration::from_millis(12);
    let t = std::time::Instant::now();
    let mut spawned_now = 0usize;
    let trace = std::env::var("NVRE_TRACE").is_ok();
    let refs = session.cell_refs().to_vec();
    // Resolve the whole cell's model paths in one batch, so the load order is
    // built once rather than once per base object. Only the references still to
    // be drawn are asked for: the rest are already spawned or have failed.
    let wanted: Vec<u32> = refs
        .iter()
        .filter(|r| {
            !spawned.refs.contains_key(&r.form_id)
                && !spawned.failed.contains(&r.form_id)
                && r.position.is_some()
        })
        .filter_map(|r| r.base.as_deref().and_then(parse_formid))
        .collect();
    let t_paths = std::time::Instant::now();
    let paths = session.model_paths_for(&wanted);
    if trace {
        bevy::log::info!(
            "draw_cell: resolved {} model paths in {:?}",
            wanted.len(),
            t_paths.elapsed()
        );
    }
    let mut resolved: HashMap<u32, String> = HashMap::new();
    for (base, path) in wanted.iter().zip(paths) {
        if let Some(p) = path {
            resolved.insert(*base, p);
        }
    }
    // `spawn_nif` reads and parses the file every time it is called, and a cell
    // reuses the same handful of models across dozens of references. So each
    // distinct model is spawned once and its entity cloned for the rest, which
    // shares the mesh and material assets rather than rebuilding them.
    let mut spawned_models: HashMap<String, Entity> = HashMap::new();
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
        spawned_now += 1;
        // Stop once the frame has spent its budget. The first model of a frame is
        // always allowed through, so a cell with one expensive model still starts
        // filling in.
        if spawned_now > 1 && t.elapsed() >= FRAME_BUDGET {
            break;
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
        let Some(model_path) = resolved.get(&base_fid).cloned() else {
            counts.no_model += 1;
            spawned.failed.insert(r.form_id);
            continue;
        };
        let t_model = std::time::Instant::now();
        // One entity per distinct model. A reference whose model is already up
        // gets a clone of that entity, which shares its mesh and material assets.
        let root = match spawned_models.get(&model_path) {
            Some(&existing) => {
                // Copy the model's components onto a new entity: `clone_and_spawn`
                // walks the source's components, which is what shares the mesh and
                // material assets rather than rebuilding them.
                let clone = models.commands.entity(existing).clone_and_spawn().id();
                clone
            }
            None => {
                let Ok(report) = models.spawn_nif(&model_path, None) else {
                    counts.spawn_failed += 1;
                    spawned.failed.insert(r.form_id);
                    continue;
                };
                let Some(root) = report.root else {
                    counts.no_root += 1;
                    spawned.failed.insert(r.form_id);
                    continue;
                };
                if trace {
                    bevy::log::info!("draw_cell: {model_path} took {:?}", t_model.elapsed());
                }
                spawned_models.insert(model_path.clone(), root);
                root
            }
        };
        // Composed with the game's own Z-up to Y-up rotation, the way the game's
        // viewer places a reference under its world root. The reference's own
        // rotation goes in too: a door or a road is not axis-aligned in the file,
        // and drawing it as if it were is most of "positioned wrong".
        let rotation = r.rotation.unwrap_or([0.0, 0.0, 0.0]);
        let t = game_to_bevy() * ref_transform(position, rotation, 1.0);
        // Queued on the asset layer's own `Commands`: `spawn_nif` spawns the root
        // through those, so the entity does not exist yet in this system's own
        // queue and inserting through a separate `Commands` would fail with
        // "Entity not yet spawned".
        models.commands.entity(root).insert((
            Name::new(
                r.edid
                    .clone()
                    .unwrap_or_else(|| format!("{:08X}", r.form_id)),
            ),
            RefEntity { form_id: r.form_id },
            t,
            Visibility::default(),
        ));
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
    nif_materials: Query<Entity, With<MeshMaterial3d<nvf_assets::NifMaterial>>>,
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
            .remove::<MeshMaterial3d<nvf_assets::NifMaterial>>()
            .insert(MeshMaterial3d(flat.clone()));
        swapped += 1;
    }
    bevy::log::info!("flat_material_diagnostic: swapped {swapped} meshes to flat magenta");
}

/// Click a reference to select it.
pub fn pick(
    mut session: ResMut<crate::session::Session>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut orbit: ResMut<Orbit>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
    windows: Query<&Window>,
    refs: Query<(&RefEntity, &GlobalTransform)>,
    mut clicks: Local<ClickState>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok((camera, cam_t)) = cameras.single() else {
        return;
    };
    let Some(window) = windows.iter().next() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_t, cursor) else {
        return;
    };

    // Nearest reference whose model the ray passes near. A precise mesh raycast
    // needs the BVH the asset layer builds; distance to the reference's origin is
    // enough to pick in a cell, and never picks a reference behind another.
    let mut best: Option<(f32, u32)> = None;
    for (r, t) in &refs {
        let to = t.translation() - ray.origin;
        let along = to.dot(*ray.direction);
        if along <= 0.0 {
            continue;
        }
        let perp = (to - *ray.direction * along).length();
        let score = perp + along * 0.001;
        if best.is_none_or(|(d, _)| score < d) {
            best = Some((score, r.form_id));
        }
    }
    let Some((_, fid)) = best else { return };
    session.select(fid);

    // A double-click frames the reference, the way GECK does: the camera moves to
    // look at it. The click has to be on the same reference both times and close
    // enough in time, so an accidental second click does not move the view.
    if clicks.register(fid) {
        frame_reference(&mut orbit, &refs, fid);
    }
}

/// Point the camera at a reference.
///
/// Shared by the viewport's own double-click and the references panel's, so
/// framing an object is the same however it was asked for.
fn frame_reference(orbit: &mut Orbit, refs: &Query<(&RefEntity, &GlobalTransform)>, form_id: u32) {
    // Look at the model itself, not the cursor's ray hit, so the framing is exact
    // even when the ray only passed near the reference.
    let Some((_, t)) = refs.iter().find(|(r, _)| r.form_id == form_id) else {
        return;
    };
    orbit.target = t.translation();
    // The same distance the editor opens a cell at, so framing one reference and
    // framing the cell it is in agree. The yaw is left alone: the view swings
    // round to the object rather than snapping to a fixed angle.
    orbit.distance = Orbit::DEFAULT_DISTANCE;
    orbit.pitch = 0.0;
}

/// Frame whatever the references panel asked for.
///
/// The panel draws in its own window's schedule and the camera lives here, so the
/// intent arrives as a message rather than a call.
pub fn apply_frame_requests(
    mut orbit: ResMut<Orbit>,
    refs: Query<(&RefEntity, &GlobalTransform)>,
    mut requests: MessageReader<crate::FrameRequested>,
) {
    for r in requests.read() {
        frame_reference(&mut orbit, &refs, r.form_id);
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
/// The centre of a cell's references, in Bevy space.
///
/// The same swap [`ref_transform`] applies to a model's position, so the camera
/// looks where the models are drawn: `orbit_camera` works in Bevy space.
/// The centre of a cell's references, in the space the models are in.
///
/// A model is placed as `game_to_bevy() * ref_transform(position)`, which lands a
/// game position `(x, y, z)` at Bevy `(x, z, y)`. The camera has to target that
/// same space, or it looks at empty air: a plain `(x, -y, z)` swap here put the
/// target thousands of units from the models and rotated one axis relative to
/// them, which is what made a horizontal drag feel mirrored.
fn cell_centre(refs: &[crate::session::RefRow]) -> Option<Vec3> {
    let mut sum = Vec3::ZERO;
    let mut n = 0.0;
    for r in refs {
        let Some(p) = r.position else { continue };
        sum += Vec3::new(p[0], p[2], p[1]);
        n += 1.0;
    }
    (n > 0.0).then(|| sum / n)
}
