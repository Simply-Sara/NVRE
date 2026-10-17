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
//!  ┌─ Place ─────┐ ┌─ Status ──────┐ ╔═ Viewport ════╗
//!  │             │ │       [Save]  │ ║   3D models   ║
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
use bevy::ecs::schedule::ScheduleLabel;
use bevy::log::info;
use bevy::prelude::*;
use bevy::window::WindowRef;
use bevy_egui::{EguiGlobalSettings, EguiSchedule, PrimaryEguiContext};

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
    /// The base-object palette.
    Place,
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

/// The main window's title, and the viewport's.
pub const MAIN_TITLE: &str = "New Vegas Rust Editor";
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

pub fn spawn_viewport(mut commands: Commands) {
    let window = commands
        .spawn(Window {
            title: VIEWPORT_TITLE.into(),
            resolution: (1280, 800).into(),
            position: WindowPosition::new(IVec2::new(40, 200)),
            ..default()
        })
        .id();
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            far: 400_000.0,
            near: 1.0,
            ..default()
        }),
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
    spawn_panel::<0>(&mut commands, "Cells", 40, 40, 607, 480, Panel::Cells);
    spawn_panel::<1>(
        &mut commands,
        "References",
        667,
        40,
        380,
        640,
        Panel::References,
    );
    spawn_panel::<2>(
        &mut commands,
        "Properties",
        1067,
        40,
        440,
        520,
        Panel::Properties,
    );
    spawn_panel::<3>(&mut commands, "Place", 1067, 600, 440, 380, Panel::Place);
    spawn_panel::<4>(&mut commands, "Status", 40, 800, 760, 90, Panel::Status);
    spawn_panel::<5>(&mut commands, "Debug", 1527, 600, 360, 260, Panel::Debug);
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
