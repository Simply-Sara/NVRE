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

use bevy::camera::{OrthographicProjection, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::log::info;
use bevy::pbr::wireframe::{WireframeConfig, WireframeTopology};
use bevy::prelude::*;
use bevy::window::WindowRef;
use bevy_egui::egui;
use bevy_egui::{EguiGlobalSettings, EguiSchedule, PrimaryEguiContext};

use crate::panels::root_ui;

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
    /// The viewport's own hotbar, drawn over the 3D view.
    Viewport,
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
/// It **does** carry an egui context now, on the camera: the viewport's own
/// hotbar is drawn into it (see [`ViewportContext`]). That is one egui pass more
/// than the panels' — the difference is that this context's window *is* the
/// 3D view, so the hotbar is drawn over the models rather than beside them.
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

/// The viewport's wireframe: the one thing its hotbar controls.
///
/// Bevy's wireframe is a plugin of its own, not part of `PbrPlugin` — so a
/// wireframe button does nothing until [`WireframePlugin`] is added. It needs
/// the GPU to support `POLYGON_MODE_LINE` and immediates; on a device without
/// them it logs one warning and leaves the scene alone rather than failing.
///
/// `global` is what makes the button work without being carried to anything:
/// it puts a wireframe on **every** mesh in the scene rather than one per
/// entity, so a reference whose model loads while the wireframe is on is drawn
/// as one the same frame. The hotbar flips it; nothing else touches it.
fn wireframe_config() -> WireframeConfig {
    WireframeConfig {
        global: false,
        // 3px, not Bevy's 1px. At 1px the line is a single hardware line, thin
        // enough to vanish against a textured mesh. Above 1px Bevy takes the wide
        // path, which shades the line across its width, so a 3px line is drawn at
        // 3px rather than as a 1px line with a halo.
        default_line_width: 3.0,
        // Quads rather than triangles: a NIF shape is triangulated for the GPU, so
        // a triangle wireframe shows the triangulation's own diagonals across every
        // quad face — a mesh reads as a mass of triangles rather than as its
        // geometry. Quad topology suppresses the shared diagonal, which is what
        // makes the wireframe describe the shape instead of its tessellation.
        default_topology: WireframeTopology::Quads,
        // X-ray: the lines are drawn without depth testing, so every edge of every
        // shape shows at once. That is what a wireframe is for — seeing the whole
        // mesh, back edges included — and without it the button would produce an
        // outline of whatever happens to face the camera.
        xray_mode: true,
        default_color: Color::srgb(0.9, 0.9, 0.25),
    }
}

/// Put the viewport's wireframe config in the world, off by default.
///
/// Runs when the editor opens, so the hotbar's button has a config to flip: the
/// plugin's own `WireframeConfig` is inserted by `WireframePlugin` with Bevy's
/// defaults, and this replaces it with the viewport's — 3px quad lines, x-ray,
/// yellow — rather than leaving the button wired to someone else's defaults.
pub fn setup_viewport_wireframe(mut commands: Commands) {
    commands.insert_resource(wireframe_config());
}

/// How the viewport's hotbar is drawn.
///
/// A resource rather than a `Local` because the hotbar draws in the viewport's
/// own egui schedule while the things it toggles are render-world resources:
/// this is the single place the two halves meet, so the buttons and what they
/// switch cannot disagree about what is on.
#[derive(Resource)]
pub struct ViewportHotbar {
    /// Whether every mesh is drawn as a wireframe.
    pub wireframe: bool,
    /// Whether the viewport's ambient light is on.
    pub ambient: bool,
}

impl Default for ViewportHotbar {
    /// The ambient on and no wireframe: the viewport opens the way the editor has
    /// always drawn it, and the toggles are what change that.
    fn default() -> Self {
        Self {
            wireframe: false,
            ambient: true,
        }
    }
}

/// The egui context the viewport draws its hotbar into.
///
/// On the viewport's **camera**, not its window. bevy_egui paints a context in a
/// pass per camera — its extract system walks `Camera` and its pass runs in
/// `Core3d` — so a context with no camera is built and never drawn, and the
/// hotbar is silently absent. The viewport is the only window whose 3D contents
/// and its egui are the *same* window, which is why the context has to be its
/// own: a hotbar drawn into a panel's context would appear in that panel's
/// window, beside the 3D view rather than over it.
#[derive(Component, Clone, Copy)]
pub struct ViewportContext;

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
    let camera = commands
        .spawn((
            Camera3d::default(),
            // No tone curve. Every game the editor supports writes its
            // surfaces straight to the framebuffer in sRGB — the ambient, the
            // lights and the textures are already display values, and a tone
            // curve on top of them is what made the viewport's colours not
            // match the game's. Bevy's default is `TonyMcMapface`.
            Tonemapping::None,
            Projection::Perspective(PerspectiveProjection {
                far: 1_500_000.0,
                near: 1.0,
                ..default()
            }),
            // A free camera: it is the `Transform` that moves, driven by the viewport's
            // own input systems. There is no controller component, because there is no
            // pivot — a turn rotates the camera in place and a wheel notch walks it
            // along the line it is looking down.
            RenderTarget::Window(WindowRef::Entity(window)),
            crate::viewport::ViewportCamera,
        ))
        .id();
    // The viewport's own egui context, on the **camera**, not the window.
    // bevy_egui paints a context per camera — its extract system walks `Camera`
    // and its pass runs in `Core3d` — so a context with no camera is built and
    // never drawn, and the hotbar would be silently absent. It is added here
    // rather than in the tuple above only because that bundle is already at the
    // tuple limit.
    commands
        .entity(camera)
        .insert((EguiSchedule::new(Panel::Viewport), ViewportContext));
    info!("spawned viewport window {window}");
}

/// The viewport's hotbar, drawn over the 3D view.
///
/// A strip along the top of the viewport's own window. It is the viewport's only
/// UI: the 3D view fills the rest, and this is the frame around the top of it.
///
/// What a hotbar is for in a viewport is a display mode, not a tool — so every
/// button here switches how the scene is drawn rather than what is in it. The
/// wireframe and the ambient each write to [`ViewportHotbar`], and
/// [`apply_wireframe`] and [`apply_ambient`] read that back into the render
/// world.
pub fn viewport_hotbar(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<ViewportContext>>,
    mut hotbar: ResMut<ViewportHotbar>,
) {
    let ui = &mut root_ui(ctx.get_mut(), "viewport");
    egui::containers::panel::Panel::top("viewport_hotbar").show(ui, |ui| {
        ui.horizontal(|ui| {
            toggle(ui, &mut hotbar.wireframe, "Wireframe")
                .on_hover_text("Draw every mesh as a wireframe: the whole mesh, back edges included");
            ui.separator();
            toggle(ui, &mut hotbar.ambient, "Ambient").on_hover_text(
                "The viewport's ambient light — the only light the scene is lit by. Off leaves every surface black.",
            );
        });
    });
}

/// One hotbar button: a toggle, highlighted while it is on.
///
/// The label is what the button says and the value is what it switches, so the
/// three buttons are one widget rather than three copies of it. The highlight is
/// the value itself, which is what makes a toggle readable: a button that is on
/// looks different from one that is off, and there is no second piece of state
/// that can fall out of step with it.
fn toggle(ui: &mut egui::Ui, on: &mut bool, label: &str) -> egui::Response {
    let response = ui.selectable_label(*on, egui::RichText::new(label).strong());
    if response.clicked() {
        *on = !*on;
    }
    response
}

/// Turn the hotbar's wireframe on or off.
///
/// [`WireframeConfig::global`] is the whole switch: it puts a wireframe on every
/// mesh in the scene at once, and taking it off puts the scene back. The colour
/// and width are the ones [`wireframe_config`] names, so the config is only ever
/// changed here.
pub fn apply_wireframe(hotbar: Res<ViewportHotbar>, mut config: ResMut<WireframeConfig>) {
    config.global = hotbar.wireframe;
}

/// Turn the hotbar's ambient light off and on.
///
/// The ambient is a resource rather than an entity, so its brightness is the
/// whole of its switch: zero is the off state — Bevy's own `lighting` example
/// toggles the same resource the same way — and [`DEFAULT_AMBIENT_BRIGHTNESS`],
/// the value the editor opens with, is written back when it is turned on. That
/// is why the constant exists rather than a literal: the off state is zero and
/// the on state is this, and one name keeps the two from drifting.
pub fn apply_ambient(hotbar: Res<ViewportHotbar>, mut ambient: ResMut<GlobalAmbientLight>) {
    let wanted = if hotbar.ambient {
        DEFAULT_AMBIENT_BRIGHTNESS
    } else {
        0.0
    };
    if (ambient.brightness - wanted).abs() > f32::EPSILON {
        ambient.brightness = wanted;
    }
}

/// The brightness the viewport's ambient light opens with, in cd/m².
///
/// Named because the toggle writes it back: the off state is zero and the on
/// state is this, and a literal in each place would let the two drift.
pub const DEFAULT_AMBIENT_BRIGHTNESS: f32 = 800.0;

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
