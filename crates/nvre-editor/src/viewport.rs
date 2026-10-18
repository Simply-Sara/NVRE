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

use bevy::asset::RenderAssetUsages;
use bevy::ecs::message::MessageReader;
use bevy::input::mouse::MouseWheel;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use fnv_world::land::{
    land_texture, load_land, Land, LandParts, DEFAULT_DIFFUSE, QUAD_SIDE, SIDE, SPACING,
};
use fnv_world::transform::ref_rotation;
use nvf_assets::model::ModelCtx;
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

/// Spawn the viewport's light, ambient light and orbit state.
///
/// The camera itself is spawned by [`crate::windows::spawn_windows`], which is
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

/// Mouse drag orbits, MMB or Shift+RMB pans, WASD flies, and wheel zooms.
#[allow(clippy::too_many_arguments)]
pub fn orbit_input(
    mut orbit: ResMut<Orbit>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<bevy::input::mouse::MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    time: Res<Time>,
    session: Res<crate::session::Session>,
    refs: Query<(&RefEntity, &GlobalTransform)>,
) {
    let move_by: Vec2 = motion.read().map(|m| m.delta).sum();

    // 1. Pan with Middle Mouse Button OR Shift + Right Mouse Button
    let is_pan = buttons.pressed(MouseButton::Middle)
        || (buttons.pressed(MouseButton::Right)
            && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)));

    if is_pan {
        let right = Vec3::new(-orbit.yaw.sin(), 0.0, orbit.yaw.cos());
        let up = Vec3::Y;
        let pan_speed = orbit.distance * 0.0015;
        orbit.target += (-right * move_by.x + up * move_by.y) * pan_speed;
    } else if buttons.pressed(MouseButton::Right) {
        // 2. Orbit with Right Mouse Button
        orbit.yaw -= move_by.x * 0.005;
        orbit.pitch = (orbit.pitch - move_by.y * 0.005).clamp(-1.55, 1.55);

        // 3. Fly with WASD / QE when RMB is held down
        let forward = Vec3::new(
            orbit.yaw.cos() * orbit.pitch.cos(),
            orbit.pitch.sin(),
            orbit.yaw.sin() * orbit.pitch.cos(),
        );
        let right = Vec3::new(-orbit.yaw.sin(), 0.0, orbit.yaw.cos());
        let up = Vec3::Y;
        let mut fly_speed = (orbit.distance * 1.5 * time.delta_secs()).clamp(5.0, 50_000.0);
        if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            fly_speed *= 3.0;
        }
        if keys.pressed(KeyCode::KeyW) {
            orbit.target += forward * fly_speed;
        }
        if keys.pressed(KeyCode::KeyS) {
            orbit.target -= forward * fly_speed;
        }
        if keys.pressed(KeyCode::KeyA) {
            orbit.target -= right * fly_speed;
        }
        if keys.pressed(KeyCode::KeyD) {
            orbit.target += right * fly_speed;
        }
        if keys.pressed(KeyCode::KeyQ) {
            orbit.target -= up * fly_speed;
        }
        if keys.pressed(KeyCode::KeyE) {
            orbit.target += up * fly_speed;
        }
    }

    for w in wheel.read() {
        orbit.distance = (orbit.distance * (1.0 - w.y * 0.1)).clamp(20.0, 1_500_000.0);
    }

    // Hotkey F: Frame selected reference
    if !buttons.pressed(MouseButton::Right) && keys.just_pressed(KeyCode::KeyF) {
        if let Some(fid) = session.selected {
            frame_reference(&mut orbit, &refs, fid);
        }
    }
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
            let (dist, pitch) = if is_exterior {
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
                (Orbit::DEFAULT_DISTANCE, 0.0)
            };
            commands.insert_resource(Orbit {
                target: centre,
                distance: dist,
                pitch,
                yaw: 0.0,
            });
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
                .filter(|p| !spawned.loaded_models.contains(&fnv_vfs::normalize(p)))
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
        let key = fnv_vfs::normalize(path);
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
        let key = fnv_vfs::normalize(&path);
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

        let key = fnv_vfs::normalize(&model_path);
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
#[allow(clippy::too_many_arguments)]
pub fn pick(
    mut session: ResMut<crate::session::Session>,
    gizmo: Res<crate::gizmo::GizmoState>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut orbit: ResMut<Orbit>,
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
            frame_reference(&mut orbit, &refs, fid);
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

    let mut mat_cache: HashMap<String, Handle<nvf_assets::nif_material::NifMaterial>> =
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
    lo: &fnv_esm::LoadOrder,
    schema: &fnv_esm::schema::Schema,
    commands: &mut Commands,
    models: &mut ModelCtx,
    spawned: &mut Spawned,
    mat_cache: &mut HashMap<String, Handle<nvf_assets::nif_material::NifMaterial>>,
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
        grid.0 as f32 * fnv_world::CELL_SIZE,
        grid.1 as f32 * fnv_world::CELL_SIZE,
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
                let h = models.materials.add(nvf_assets::nif_material(mat));
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
