//! 3D Transform Gizmo and interactive manipulation for NVRE.
//!
//! Provides Translate (arrows + plane handles), Rotate (rings), and Scale (cubes),
//! matching industry-standard 3D editors (GECK, Blender, Unreal, Unity).

use crate::session::Session;
use crate::viewport::{game_to_bevy, RefEntity, ViewportCamera};
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;

/// Which transform operation the gizmo performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GizmoMode {
    #[default]
    Translate,
    Rotate,
    Scale,
}

/// Which part of the gizmo is being hovered or dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GizmoHandle {
    #[default]
    None,
    // Translation axes (Game X, Y, Z)
    TranslateX,
    TranslateY,
    TranslateZ,
    // Translation planes
    TranslatePlaneXY, // Ground plane (Game X & Y)
    TranslatePlaneXZ, // Vertical East-West
    TranslatePlaneYZ, // Vertical North-South
    // Rotation rings
    RotateX, // Pitch
    RotateY, // Roll
    RotateZ, // Yaw (Heading)
    // Scale handles
    ScaleX,
    ScaleY,
    ScaleZ,
    ScaleUniform,
}

/// Active drag state when manipulating an object.
#[derive(Clone, Debug)]
pub struct GizmoDrag {
    pub handle: GizmoHandle,
    pub start_cursor: Vec2,
    pub initial_hit_point: Vec3,
    pub origin_bevy: Vec3,
    pub start_pos_game: [f32; 3],
    pub start_rot_game: [f32; 3],
    pub start_scale_game: f32,
}

/// Resource tracking the viewport's transform gizmo.
#[derive(Resource, Default)]
pub struct GizmoState {
    pub mode: GizmoMode,
    pub hovered: GizmoHandle,
    pub dragging: Option<GizmoDrag>,
}

// Colors for the axes (standard RGB for XYZ)
const COLOR_X: Color = Color::srgb(0.9, 0.2, 0.2);
const COLOR_Y: Color = Color::srgb(0.2, 0.8, 0.2);
const COLOR_Z: Color = Color::srgb(0.2, 0.4, 0.95);
const COLOR_ACTIVE: Color = Color::srgb(1.0, 0.9, 0.1);
const COLOR_WHITE: Color = Color::srgb(0.9, 0.9, 0.9);

/// Update gizmo mode from hotkeys (W = Translate, E = Rotate, R = Scale) when RMB is not held.
pub fn gizmo_mode_hotkeys(
    mut gizmo: ResMut<GizmoState>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
) {
    if buttons.pressed(MouseButton::Right) {
        return; // Orbiting; do not switch modes mid-drag
    }
    if keys.just_pressed(KeyCode::KeyW) {
        gizmo.mode = GizmoMode::Translate;
    } else if keys.just_pressed(KeyCode::KeyE) {
        gizmo.mode = GizmoMode::Rotate;
    } else if keys.just_pressed(KeyCode::KeyR) {
        gizmo.mode = GizmoMode::Scale;
    }
}

/// Draw the 3D Gizmo for the selected reference and handle hit testing and dragging.
pub fn gizmo_system(
    mut gizmo: ResMut<GizmoState>,
    mut session: ResMut<Session>,
    mut gizmos: Gizmos,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<crate::windows::ViewportWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<ViewportCamera>>,
    mut refs_query: Query<(&RefEntity, &mut Transform)>,
) {
    let Some(selected_fid) = session.selected else {
        gizmo.hovered = GizmoHandle::None;
        gizmo.dragging = None;
        return;
    };

    let Ok((camera, cam_gt)) = cameras.single() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    let cursor = window.cursor_position();

    // Find the selected entity's current Bevy transform
    let Some((_, ref_transform)) = refs_query.iter().find(|(r, _)| r.form_id == selected_fid)
    else {
        gizmo.hovered = GizmoHandle::None;
        gizmo.dragging = None;
        return;
    };
    let origin_bevy = ref_transform.translation;

    // Scale gizmo so it maintains a constant visual size on screen
    let cam_pos = cam_gt.translation();
    let cam_dist = (cam_pos - origin_bevy).length();
    let gizmo_size = (cam_dist * 0.12).clamp(10.0, 50_000.0);

    // Bevy vectors corresponding to Game axes (Game Z-up -> Bevy Y-up):
    // Game +X (East)  -> Bevy +X
    // Game +Y (North) -> Bevy -Z
    // Game +Z (Up)    -> Bevy +Y
    let dir_x = Vec3::X;
    let dir_y = -Vec3::Z;
    let dir_z = Vec3::Y;

    let ray = cursor.and_then(|c| camera.viewport_to_world(cam_gt, c).ok());

    // 1. If currently dragging, update the object transform
    if let Some(drag) = gizmo.dragging.clone() {
        if buttons.pressed(MouseButton::Left) {
            if let Some(ray) = ray {
                apply_drag(
                    &drag,
                    ray.origin,
                    *ray.direction,
                    gizmo_size,
                    &mut session,
                    selected_fid,
                    &mut refs_query,
                );
            }
        } else {
            // Drag finished
            gizmo.dragging = None;
        }
    } else {
        // 2. Not dragging: Hit test to determine hovered handle
        let mut best_hit = (GizmoHandle::None, f32::MAX);
        if let Some(ray) = ray {
            test_gizmo_hits(
                ray.origin,
                *ray.direction,
                origin_bevy,
                gizmo_size,
                gizmo.mode,
                dir_x,
                dir_y,
                dir_z,
                &mut best_hit,
            );
        }
        gizmo.hovered = best_hit.0;

        // If left mouse button was just pressed on a handle, begin dragging
        if buttons.just_pressed(MouseButton::Left) && gizmo.hovered != GizmoHandle::None {
            if let Some((pos, rot, scale)) = session.selected_placement() {
                let init_hit = if let Some(r) = ray {
                    let plane_norm = match gizmo.hovered {
                        GizmoHandle::TranslateX => (cam_pos - origin_bevy)
                            .cross(dir_x)
                            .cross(dir_x)
                            .normalize_or(Vec3::Y),
                        GizmoHandle::TranslateY => (cam_pos - origin_bevy)
                            .cross(dir_y)
                            .cross(dir_y)
                            .normalize_or(Vec3::Y),
                        GizmoHandle::TranslateZ => (cam_pos - origin_bevy)
                            .cross(dir_z)
                            .cross(dir_z)
                            .normalize_or(Vec3::Y),
                        GizmoHandle::TranslatePlaneXY | GizmoHandle::RotateZ => dir_z,
                        GizmoHandle::TranslatePlaneXZ | GizmoHandle::RotateY => dir_y,
                        GizmoHandle::TranslatePlaneYZ | GizmoHandle::RotateX => dir_x,
                        _ => (cam_pos - origin_bevy).normalize_or(Vec3::Y),
                    };
                    ray_plane_intersection(r.origin, *r.direction, origin_bevy, plane_norm)
                        .map(|(p, _)| p)
                        .unwrap_or(origin_bevy)
                } else {
                    origin_bevy
                };
                gizmo.dragging = Some(GizmoDrag {
                    handle: gizmo.hovered,
                    start_cursor: cursor.unwrap_or(Vec2::ZERO),
                    initial_hit_point: init_hit,
                    origin_bevy,
                    start_pos_game: pos,
                    start_rot_game: rot,
                    start_scale_game: scale,
                });
            }
        }
    }

    // 3. Render the gizmo in the 3D viewport
    let active_handle = gizmo
        .dragging
        .as_ref()
        .map(|d| d.handle)
        .unwrap_or(gizmo.hovered);
    draw_gizmo_visuals(
        &mut gizmos,
        origin_bevy,
        gizmo_size,
        gizmo.mode,
        active_handle,
        dir_x,
        dir_y,
        dir_z,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_gizmo_visuals(
    gizmos: &mut Gizmos,
    origin: Vec3,
    size: f32,
    mode: GizmoMode,
    active: GizmoHandle,
    dx: Vec3,
    dy: Vec3,
    dz: Vec3,
) {
    match mode {
        GizmoMode::Translate => {
            // Axis Arrows
            let col_x = if active == GizmoHandle::TranslateX {
                COLOR_ACTIVE
            } else {
                COLOR_X
            };
            let col_y = if active == GizmoHandle::TranslateY {
                COLOR_ACTIVE
            } else {
                COLOR_Y
            };
            let col_z = if active == GizmoHandle::TranslateZ {
                COLOR_ACTIVE
            } else {
                COLOR_Z
            };

            gizmos.arrow(origin, origin + dx * size, col_x);
            gizmos.arrow(origin, origin + dy * size, col_y);
            gizmos.arrow(origin, origin + dz * size, col_z);

            // Plane square handles (offset from origin)
            let plane_size = size * 0.35;
            let col_xy = if active == GizmoHandle::TranslatePlaneXY {
                COLOR_ACTIVE
            } else {
                Color::srgba(0.2, 0.8, 0.9, 0.8)
            };
            let col_xz = if active == GizmoHandle::TranslatePlaneXZ {
                COLOR_ACTIVE
            } else {
                Color::srgba(0.9, 0.8, 0.2, 0.8)
            };
            let col_yz = if active == GizmoHandle::TranslatePlaneYZ {
                COLOR_ACTIVE
            } else {
                Color::srgba(0.9, 0.4, 0.2, 0.8)
            };

            // Ground Plane (XY in game = X and -Z in Bevy)
            gizmos.line(
                origin + dx * plane_size,
                origin + (dx + dy) * plane_size,
                col_xy,
            );
            gizmos.line(
                origin + dy * plane_size,
                origin + (dx + dy) * plane_size,
                col_xy,
            );

            // Vertical Plane XZ (Game X and Z = Bevy X and Y)
            gizmos.line(
                origin + dx * plane_size,
                origin + (dx + dz) * plane_size,
                col_xz,
            );
            gizmos.line(
                origin + dz * plane_size,
                origin + (dx + dz) * plane_size,
                col_xz,
            );

            // Vertical Plane YZ (Game Y and Z = Bevy -Z and Y)
            gizmos.line(
                origin + dy * plane_size,
                origin + (dy + dz) * plane_size,
                col_yz,
            );
            gizmos.line(
                origin + dz * plane_size,
                origin + (dy + dz) * plane_size,
                col_yz,
            );
        }
        GizmoMode::Rotate => {
            let col_x = if active == GizmoHandle::RotateX {
                COLOR_ACTIVE
            } else {
                COLOR_X
            };
            let col_y = if active == GizmoHandle::RotateY {
                COLOR_ACTIVE
            } else {
                COLOR_Y
            };
            let col_z = if active == GizmoHandle::RotateZ {
                COLOR_ACTIVE
            } else {
                COLOR_Z
            };

            let radius = size * 0.9;
            // X Ring (Pitch): normal is Game X = Bevy X
            let iso_x = Isometry3d::new(origin, Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
            gizmos.circle(iso_x, radius, col_x);

            // Y Ring (Roll): normal is Game Y = Bevy -Z
            let iso_y = Isometry3d::new(origin, Quat::IDENTITY);
            gizmos.circle(iso_y, radius, col_y);

            // Z Ring (Yaw / Heading): normal is Game Z = Bevy Y
            let iso_z = Isometry3d::new(origin, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
            gizmos.circle(iso_z, radius, col_z);
        }
        GizmoMode::Scale => {
            let col_x = if active == GizmoHandle::ScaleX {
                COLOR_ACTIVE
            } else {
                COLOR_X
            };
            let col_y = if active == GizmoHandle::ScaleY {
                COLOR_ACTIVE
            } else {
                COLOR_Y
            };
            let col_z = if active == GizmoHandle::ScaleZ {
                COLOR_ACTIVE
            } else {
                COLOR_Z
            };
            let col_u = if active == GizmoHandle::ScaleUniform {
                COLOR_ACTIVE
            } else {
                COLOR_WHITE
            };

            let cube_size = size * 0.12;

            // Axis lines
            gizmos.line(origin, origin + dx * size, col_x);
            gizmos.line(origin, origin + dy * size, col_y);
            gizmos.line(origin, origin + dz * size, col_z);

            // Endpoint Cubes
            gizmos.cube(
                Transform::from_translation(origin + dx * size).with_scale(Vec3::splat(cube_size)),
                col_x,
            );
            gizmos.cube(
                Transform::from_translation(origin + dy * size).with_scale(Vec3::splat(cube_size)),
                col_y,
            );
            gizmos.cube(
                Transform::from_translation(origin + dz * size).with_scale(Vec3::splat(cube_size)),
                col_z,
            );

            // Center Uniform Scale Cube
            gizmos.cube(
                Transform::from_translation(origin).with_scale(Vec3::splat(cube_size * 0.8)),
                col_u,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn test_gizmo_hits(
    ray_origin: Vec3,
    ray_dir: Vec3,
    origin: Vec3,
    size: f32,
    mode: GizmoMode,
    dx: Vec3,
    dy: Vec3,
    dz: Vec3,
    best_hit: &mut (GizmoHandle, f32),
) {
    let thresh = size * 0.12;

    match mode {
        GizmoMode::Translate => {
            // Check axis arrows
            for (handle, axis) in [
                (GizmoHandle::TranslateX, dx),
                (GizmoHandle::TranslateY, dy),
                (GizmoHandle::TranslateZ, dz),
            ] {
                let (dist, t_ray, s_axis) =
                    ray_segment_closest(ray_origin, ray_dir, origin, origin + axis * size);
                if dist < thresh
                    && (0.0..=1.0).contains(&s_axis)
                    && t_ray > 0.0
                    && t_ray < best_hit.1
                {
                    *best_hit = (handle, t_ray);
                }
            }

            // Check plane squares
            let plane_size = size * 0.35;
            for (handle, u, v, norm) in [
                (GizmoHandle::TranslatePlaneXY, dx, dy, dz),
                (GizmoHandle::TranslatePlaneXZ, dx, dz, dy),
                (GizmoHandle::TranslatePlaneYZ, dy, dz, dx),
            ] {
                if let Some((hit, t)) = ray_plane_intersection(ray_origin, ray_dir, origin, norm) {
                    let rel = hit - origin;
                    let proj_u = rel.dot(u);
                    let proj_v = rel.dot(v);
                    if proj_u >= 0.0
                        && proj_u <= plane_size
                        && proj_v >= 0.0
                        && proj_v <= plane_size
                        && t < best_hit.1
                    {
                        *best_hit = (handle, t);
                    }
                }
            }
        }
        GizmoMode::Rotate => {
            let radius = size * 0.9;
            for (handle, norm) in [
                (GizmoHandle::RotateX, dx),
                (GizmoHandle::RotateY, dy),
                (GizmoHandle::RotateZ, dz),
            ] {
                if let Some((hit, t)) = ray_plane_intersection(ray_origin, ray_dir, origin, norm) {
                    let d = (hit - origin).length();
                    if (d - radius).abs() < thresh && t < best_hit.1 {
                        *best_hit = (handle, t);
                    }
                }
            }
        }
        GizmoMode::Scale => {
            let cube_size = size * 0.12;
            // Center cube (uniform)
            if let Some(t) = ray_aabb_intersection(
                ray_origin,
                ray_dir,
                origin - Vec3::splat(cube_size * 0.4),
                origin + Vec3::splat(cube_size * 0.4),
            ) {
                if t < best_hit.1 {
                    *best_hit = (GizmoHandle::ScaleUniform, t);
                }
            }
            // Axis cubes
            for (handle, axis) in [
                (GizmoHandle::ScaleX, dx),
                (GizmoHandle::ScaleY, dy),
                (GizmoHandle::ScaleZ, dz),
            ] {
                let p = origin + axis * size;
                if let Some(t) = ray_aabb_intersection(
                    ray_origin,
                    ray_dir,
                    p - Vec3::splat(cube_size * 0.5),
                    p + Vec3::splat(cube_size * 0.5),
                ) {
                    if t < best_hit.1 {
                        *best_hit = (handle, t);
                    }
                }
            }
        }
    }
}

fn apply_drag(
    drag: &GizmoDrag,
    ray_origin: Vec3,
    ray_dir: Vec3,
    gizmo_size: f32,
    session: &mut Session,
    fid: u32,
    refs_query: &mut Query<(&RefEntity, &mut Transform)>,
) {
    let dx = Vec3::X;
    let dy = -Vec3::Z;
    let dz = Vec3::Y;

    match drag.handle {
        GizmoHandle::TranslateX | GizmoHandle::TranslateY | GizmoHandle::TranslateZ => {
            let axis = match drag.handle {
                GizmoHandle::TranslateX => dx,
                GizmoHandle::TranslateY => dy,
                _ => dz,
            };
            // Plane passing through origin facing camera
            let cam_vec = (ray_origin - drag.origin_bevy).normalize_or(Vec3::Y);
            let plane_norm = (cam_vec.cross(axis)).cross(axis).normalize_or(cam_vec);
            if let Some((hit, _)) =
                ray_plane_intersection(ray_origin, ray_dir, drag.origin_bevy, plane_norm)
            {
                let delta = (hit - drag.initial_hit_point).dot(axis);
                let mut new_pos = drag.start_pos_game;
                match drag.handle {
                    GizmoHandle::TranslateX => new_pos[0] += delta,
                    GizmoHandle::TranslateY => new_pos[1] -= delta, // In Game space, Bevy -Z is Game +Y
                    _ => new_pos[2] += delta,
                }
                session.move_selected(new_pos);
                update_ref_entity(
                    refs_query,
                    fid,
                    new_pos,
                    drag.start_rot_game,
                    drag.start_scale_game,
                );
            }
        }
        GizmoHandle::TranslatePlaneXY
        | GizmoHandle::TranslatePlaneXZ
        | GizmoHandle::TranslatePlaneYZ => {
            let norm = match drag.handle {
                GizmoHandle::TranslatePlaneXY => dz,
                GizmoHandle::TranslatePlaneXZ => dy,
                _ => dx,
            };
            if let Some((hit, _)) =
                ray_plane_intersection(ray_origin, ray_dir, drag.origin_bevy, norm)
            {
                let bevy_delta = hit - drag.initial_hit_point;
                let new_pos = [
                    drag.start_pos_game[0] + bevy_delta.x,
                    drag.start_pos_game[1] - bevy_delta.z,
                    drag.start_pos_game[2] + bevy_delta.y,
                ];
                session.move_selected(new_pos);
                update_ref_entity(
                    refs_query,
                    fid,
                    new_pos,
                    drag.start_rot_game,
                    drag.start_scale_game,
                );
            }
        }
        GizmoHandle::RotateX | GizmoHandle::RotateY | GizmoHandle::RotateZ => {
            let norm = match drag.handle {
                GizmoHandle::RotateX => dx,
                GizmoHandle::RotateY => dy,
                _ => dz,
            };
            if let Some((hit, _)) =
                ray_plane_intersection(ray_origin, ray_dir, drag.origin_bevy, norm)
            {
                let v0 = (drag.initial_hit_point - drag.origin_bevy).normalize_or(Vec3::X);
                let v1 = (hit - drag.origin_bevy).normalize_or(Vec3::X);
                let angle = v0.cross(v1).dot(norm).atan2(v0.dot(v1));
                let mut new_rot = drag.start_rot_game;
                match drag.handle {
                    GizmoHandle::RotateX => new_rot[0] += angle,
                    GizmoHandle::RotateY => new_rot[1] -= angle,
                    _ => new_rot[2] -= angle,
                }
                session.rotate_selected(new_rot);
                update_ref_entity(
                    refs_query,
                    fid,
                    drag.start_pos_game,
                    new_rot,
                    drag.start_scale_game,
                );
            }
        }
        GizmoHandle::ScaleX
        | GizmoHandle::ScaleY
        | GizmoHandle::ScaleZ
        | GizmoHandle::ScaleUniform => {
            let axis = match drag.handle {
                GizmoHandle::ScaleX => dx,
                GizmoHandle::ScaleY => dy,
                GizmoHandle::ScaleZ => dz,
                _ => Vec3::ONE.normalize(),
            };
            let cam_vec = (ray_origin - drag.origin_bevy).normalize_or(Vec3::Y);
            let plane_norm = (cam_vec.cross(axis)).cross(axis).normalize_or(cam_vec);
            if let Some((hit, _)) =
                ray_plane_intersection(ray_origin, ray_dir, drag.origin_bevy, plane_norm)
            {
                let delta = (hit - drag.initial_hit_point).dot(axis);
                let factor = (1.0 + delta / gizmo_size).max(0.01);
                let new_scale = drag.start_scale_game * factor;
                session.scale_selected(new_scale);
                update_ref_entity(
                    refs_query,
                    fid,
                    drag.start_pos_game,
                    drag.start_rot_game,
                    new_scale,
                );
            }
        }
        GizmoHandle::None => {}
    }
}

fn update_ref_entity(
    refs_query: &mut Query<(&RefEntity, &mut Transform)>,
    fid: u32,
    pos: [f32; 3],
    rot: [f32; 3],
    scale: f32,
) {
    if let Some((_, mut tr)) = refs_query.iter_mut().find(|(r, _)| r.form_id == fid) {
        *tr = game_to_bevy() * crate::viewport::ref_transform(pos, rot, scale);
    }
}

fn ray_segment_closest(ray_orig: Vec3, ray_dir: Vec3, p0: Vec3, p1: Vec3) -> (f32, f32, f32) {
    let seg_vec = p1 - p0;
    let seg_len2 = seg_vec.length_squared();
    if seg_len2 < 1e-6 {
        let d = (p0 - ray_orig).cross(ray_dir).length();
        return (d, (p0 - ray_orig).dot(ray_dir), 0.0);
    }
    let r_dot_s = ray_dir.dot(seg_vec);
    let r_dot_w = ray_dir.dot(ray_orig - p0);
    let s_dot_w = seg_vec.dot(ray_orig - p0);

    let denom = seg_len2 - r_dot_s * r_dot_s;
    let s = if denom.abs() > 1e-6 {
        (r_dot_s * r_dot_w - s_dot_w) / denom
    } else {
        0.0
    }
    .clamp(0.0, 1.0);

    let pt_on_seg = p0 + seg_vec * s;
    let t = (pt_on_seg - ray_orig).dot(ray_dir);
    let pt_on_ray = ray_orig + ray_dir * t.max(0.0);
    let dist = (pt_on_seg - pt_on_ray).length();
    (dist, t, s)
}

fn ray_plane_intersection(
    ray_orig: Vec3,
    ray_dir: Vec3,
    plane_pt: Vec3,
    plane_norm: Vec3,
) -> Option<(Vec3, f32)> {
    let denom = plane_norm.dot(ray_dir);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = (plane_pt - ray_orig).dot(plane_norm) / denom;
    if t < 0.0 {
        return None;
    }
    Some((ray_orig + ray_dir * t, t))
}

fn ray_aabb_intersection(ray_orig: Vec3, ray_dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut tmin = (min.x - ray_orig.x) / ray_dir.x;
    let mut tmax = (max.x - ray_orig.x) / ray_dir.x;
    if tmin > tmax {
        std::mem::swap(&mut tmin, &mut tmax);
    }

    let mut tymin = (min.y - ray_orig.y) / ray_dir.y;
    let mut tymax = (max.y - ray_orig.y) / ray_dir.y;
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

    let mut tzmin = (min.z - ray_orig.z) / ray_dir.z;
    let mut tzmax = (max.z - ray_orig.z) / ray_dir.z;
    if tzmin > tzmax {
        std::mem::swap(&mut tzmin, &mut tzmax);
    }

    if tmin > tzmax || tzmin > tmax {
        return None;
    }
    if tzmin > tmin {
        tmin = tzmin;
    }
    if tmin < 0.0 {
        return None;
    }
    Some(tmin)
}
