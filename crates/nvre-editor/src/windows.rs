//! The editor's windows.
//!
//! GECK keeps its panels in separate, movable windows, and this does the same:
//! each panel is its own Bevy window with its own egui context. A window's
//! contents are drawn by a system registered against that window's
//! [`EguiSchedule`], so every panel is independent of the others — they can be
//! moved, resized and closed separately.
//!
//! ```text
//!  ┌─ Cells ─────┐ ┌─ References ──┐ ┌─ Properties ──┐
//!  │             │ │               │ │               │
//!  └─────────────┘ └───────────────┘ └───────────────┘
//!  ┌─ Objects ───┐ ┌─ Status ──────┐ ╔═ Viewport ════╗
//!  │ categories  │ │       [Save]  │ ║   3D models   ║
//!  └─────────────┘ └───────────────┘ ╚═══════════════╝
//! ```
//!
//! The **main** window is the app's primary window. That is not a preference:
//! winit creates the primary window during its `resumed` event, and a window
//! spawned later only reaches the OS through an event-loop proxy round trip that
//! needs the primary window to exist first.
//!
//! The viewport is its own window, spawned after that, with a camera whose
//! `RenderTarget` names it explicitly. Naming the target rather than relying on
//! the primary-window fallback is what makes the camera's destination
//! unambiguous when several windows are open.

use bevy::camera::OrthographicProjection;
use bevy::camera::RenderTarget;
use bevy::camera_controller::pan_orbit_camera::controller::component::PanOrbitCamera;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::log::info;
use bevy::prelude::*;
use bevy::window::WindowRef;
use bevy_egui::{EguiGlobalSettings, EguiSchedule, PrimaryEguiContext};

/// Make every scroll bar in every window visible at its full width.
///
/// This has to visit each `EguiContext` in turn, because the app has one per
/// window — the primary context for the launcher and the main window, and one
/// `PanelContext<N>` per panel — and an egui style lives on its context. Setting
/// it on one context reaches only that window: the launcher's bars were widened
/// while every panel's stayed thin and transparent.
///
/// Runs every frame rather than once, so a context spawned later (a panel window
/// opened after startup) is covered on its first frame.
///
/// The two things this changes, both of which hide a floating scroll bar:
///
/// * **Width** — egui's idle bar is 2px and grows to `bar_width` only under the
///   pointer, so `floating_width` is set to `bar_width`.
/// * **Opacity** — egui's default `floating()` preset sets the `dormant_*`
///   opacities to `0.0`, so the bar is transparent once the pointer leaves. A
///   full-width bar at zero opacity is still invisible.
///
/// Both are set to the level egui itself uses while the pointer is over the
/// scroll area but not the bar: a visible grey, rather than the solid handle of
/// a direct hover.
pub fn widen_scroll_bars(mut contexts: Query<&mut bevy_egui::EguiContext>) {
    for mut ctx in &mut contexts {
        ctx.get_mut().all_styles_mut(|style| {
            let scroll = &mut style.spacing.scroll;
            scroll.floating_width = scroll.bar_width;
            scroll.dormant_handle_opacity = scroll.active_handle_opacity;
            scroll.dormant_background_opacity = scroll.active_background_opacity;
        });
    }
}

/// A panel window: its schedule, and the marker that ties it to its egui
/// context.
///
/// The schedule label is what makes a window's egui pass run, and the marker is
/// what lets the drawing system pick its own context out of the world — one
/// context per window is what keeps the panels' widget state separate.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Panel {
    /// The cell browser.
    Cells,
    /// The references in the open cell.
    References,
    /// The property grid for the selection.
    Properties,
    /// The base-object palette, grouped into GECK's categories.
    Objects,
    /// The status line and the save button.
    Status,
    /// The camera's state, read and written while the editor runs.
    Debug,
}

/// Marks the egui context that belongs to one panel's window.
///
/// A marker per window rather than one shared context: each window's widgets
/// then have their own ids, focus and scroll state, so the cell browser's scroll
/// position survives while the reference list scrolls.
///
/// The const parameter distinguishes the windows, which a plain unit struct
/// cannot: five `PanelContext` components would be one type, and a `Single`
/// query could not tell them apart.
#[derive(Component, Clone, Copy)]
pub struct PanelContext<const P: u8>;

/// The main window's title comes from the open game's profile
/// ([`crate::game::GameProfile::editor_name`]); this is the viewport's.
pub const VIEWPORT_TITLE: &str = "Viewport";

/// Spawn the viewport window, with a camera that draws into it.
///
/// Runs from `Startup` so the window exists before the first frame. The camera's
/// `RenderTarget` names the window explicitly: with several windows open, a
/// camera with no target falls back to the primary window, and that fallback is
/// what the viewport used to depend on.
///
/// It carries **no** egui context. The viewport draws 3D models and nothing else,
/// so it needs no UI; giving it a context would put an egui pass in the loop with
/// no window of its own to render into, and bevy_egui would drop that pass's
/// output unapplied and panic.
/// Attach the primary egui context to the main window.
///
/// Runs from `Startup`, before anything else: the launcher draws into the primary
/// context, and until a plugin is opened there is no other context to draw into.
/// `spawn_windows` turns off automatic context creation when the editor starts,
/// which is what keeps the panels' contexts separate from this one.
pub fn setup_primary(mut commands: Commands) {
    commands.spawn((Camera2d, PrimaryEguiContext));
}

/// Marker component for the 3D viewport window.
#[derive(Component)]
pub struct ViewportWindow;

pub fn spawn_viewport(mut commands: Commands) {
    let window = commands
        .spawn((
            Window {
                title: VIEWPORT_TITLE.into(),
                resolution: (1280, 800).into(),
                position: WindowPosition::new(IVec2::new(40, 200)),
                ..default()
            },
            ViewportWindow,
        ))
        .id();
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            far: 1_500_000.0,
            near: 1.0,
            ..default()
        }),
        // The camera's own controller: it owns the pose, and the viewport tells it
        // which motion to start and feeds it the mouse.
        PanOrbitCamera {
            // The anchor depth is how far away the thing being orbited and zoomed
            // at is, and it drives the zoom speed. The default is two metres,
            // which is right for a metre-scale scene and far too close for NVRE:
            // game units are ~4.19 cm, so a cell's contents sit thousands of units
            // out. Without this, the first wheel notch would zoom by a fraction of
            // a unit and the view would appear stuck.
            last_anchor_depth: -(crate::viewport::ViewportFocus::DEFAULT_DISTANCE as f64),
            // The controller moves the near plane with the anchor depth, which is
            // right for a metre-scale scene and wrong here: at a worldspace's
            // viewing distance it would push the near plane out by thousands of
            // units and clip whatever the camera is next to. Bounding it keeps a
            // near plane of about a unit, as the camera was spawned with.
            perspective: bevy::camera_controller::pan_orbit_camera::controller::projections::PerspectiveSettings {
                near_clip_limits: 1.0..1_000.0,
                ..default()
            },
            ..default()
        },
        RenderTarget::Window(WindowRef::Entity(window)),
        crate::viewport::ViewportCamera,
    ));
    info!("spawned viewport window {window}");
}

/// How many panel windows `spawn_windows` opens.
const WINDOW_COUNT: usize = 6;

/// Spawn the panel windows, each with its own egui context.
pub fn spawn_windows(mut commands: Commands, mut settings: ResMut<EguiGlobalSettings>) {
    // Without this, bevy_egui attaches a context to the first camera it sees and
    // every window would share it.
    settings.auto_create_primary_context = false;

    // The Cells panel's table needs the width: Editor ID, Form ID, Name and
    // Location side by side. The panels to its right start after it, so the
    // table's columns are not hidden under the References panel.
    //
    // The References panel's own table carries five columns — Editor ID, Form
    // ID, Type, Owner and Lock — and needs the room for them: it sits next to
    // the Cells panel rather than under it, and the Properties and Objects
    // panels follow it along.
    spawn_panel::<0>(&mut commands, "Cells", 40, 40, 607, 480, Panel::Cells);
    spawn_panel::<1>(
        &mut commands,
        "References",
        667,
        40,
        620,
        640,
        Panel::References,
    );
    spawn_panel::<2>(
        &mut commands,
        "Properties",
        1307,
        40,
        560,
        520,
        Panel::Properties,
    );
    spawn_panel::<3>(
        &mut commands,
        "Objects",
        1307,
        600,
        560,
        380,
        Panel::Objects,
    );
    spawn_panel::<4>(&mut commands, "Status", 40, 800, 760, 90, Panel::Status);
    spawn_panel::<5>(&mut commands, "Debug", 1877, 600, 360, 260, Panel::Debug);
    info!("spawned {} panel windows", WINDOW_COUNT);
}

/// One panel window: a Bevy window, a 2D camera that renders to it, the egui
/// schedule its UI runs in, and the marker identifying its context.
fn spawn_panel<const P: u8>(
    commands: &mut Commands,
    title: &str,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    schedule: Panel,
) {
    let window = commands
        .spawn(Window {
            title: title.to_owned(),
            resolution: (w, h).into(),
            position: WindowPosition::new(IVec2::new(x, y)),
            ..default()
        })
        .id();
    commands.spawn((
        Camera2d,
        // An orthographic projection, like bevy_egui's own examples give a 2D
        // camera. Without one the camera has no projection of its own, and
        // whether it reports a viewport rect at all is up to defaults.
        Projection::Orthographic(OrthographicProjection::default_2d()),
        RenderTarget::Window(WindowRef::Entity(window)),
        EguiSchedule::new(schedule),
        PanelContext::<P>,
    ));
    info!("spawned panel window {title} ({window})");
}
