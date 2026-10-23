//! Does the drag trace actually emit, and does it name the right moments?
//!
//! The trace is the instrument for "the object is placed but never shows up",
//! so an instrument that silently prints nothing is worse than none: it looks
//! like evidence that a state change never happened.
//!
//! Three things are pinned here: that the flag works, that the prefix a reader
//! greps for is the one the source emits, and — the important one — that the
//! real systems write a line when a drag is in flight and write nothing when
//! they are idle. The last of those is what makes the trace trustworthy: a
//! trace that is quiet during a drag is indistinguishable from a drag that
//! never happened.

use bevy::camera::RenderTarget;
use bevy::math::DVec2;
use bevy::prelude::*;
use nvre_editor::object_drag::{
    begin_ghost, draw_ghost, trace, update_ghost, DragGhost, DragStarted, DraggedObject,
};
use nvre_editor::viewport::ViewportCamera;
use nvre_editor::windows::ViewportWindow;
use std::sync::{Mutex, OnceLock};

/// The prefix every drag line carries, so a grep for it returns the whole
/// gesture rather than one system's share of it.
pub const DRAG_PREFIX: &str = "[drag]";

/// Lines the capturing subscriber has seen across the whole test binary.
fn captured() -> &'static Mutex<Vec<String>> {
    static LINES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    LINES.get_or_init(|| Mutex::new(Vec::new()))
}

/// Held for the duration of any test that reads the capture buffer.
///
/// The subscriber is process-global, so the buffer it writes is too: two tests
/// that run concurrently share it, and one test's `clear()` would erase the
/// other's lines mid-assert. A real logger is global in the binary under test as
/// well, so this is the same rule a reader of the trace would have to follow.
static CAPTURE_LOCK: Mutex<()> = Mutex::new(());

/// A guard that empties the buffer on construction and on drop, holds the lock
/// for its whole lifetime, and owns the `NVRE_DRAG_TRACE` environment variable
/// for that same lifetime.
///
/// Two process-global things have to be held together, not one. The capture
/// buffer is global because the logger is, so a concurrent test's lines would
/// otherwise land mid-assert; and the trace flag is global because it is an
/// environment variable, so a test that clears it would turn the trace off
/// underneath a test that depends on it. Holding both under one lock is what
/// makes the tests order-independent.
struct CaptureGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl CaptureGuard {
    /// Take the lock, turn the trace on, and empty the buffer.
    ///
    /// Setting the flag here rather than in the test is what makes the
    /// serialization real: the flag and the buffer are both process-global, and
    /// a test that sets one without holding the lock can still lose either to a
    /// concurrent test that is between its own set and its assert.
    fn new() -> Self {
        // The lock is a `static` promoted to `'static` by the borrow, so the
        // guard can hold it without either a lifetime parameter or a leak. One
        // lock per process, which is the point: every test that takes it
        // serializes against every other.
        let guard = CAPTURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("NVRE_DRAG_TRACE", "1");
        clear();
        Self { _lock: guard }
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        clear();
        std::env::remove_var("NVRE_DRAG_TRACE");
    }
}

/// A `log` subscriber that keeps the formatted message of every record.
///
/// A `static` reference rather than a `Box`: `log::set_global_default` needs the
/// crate's `std` feature, which the shared `log` build in this tree does not
/// enable, and `set_logger` is the no-`std` equivalent that is.
static CAPTURE: Capture = Capture;

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        captured()
            .lock()
            .unwrap()
            .push(format!("{}", record.args()));
    }

    fn flush(&self) {}
}

fn install() {
    // `set_logger` succeeds once per process; a later call fails, which is what
    // makes this safe from every test in the binary. `set_max_level` is a plain
    // atomic and is not gated on `std`.
    let _ = log::set_logger(&CAPTURE);
    log::set_max_level(log::LevelFilter::Info);
}

fn clear() {
    captured().lock().unwrap().clear();
}

fn drag_lines() -> Vec<String> {
    captured()
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains(DRAG_PREFIX))
        .cloned()
        .collect()
}

#[test]
fn the_trace_flag_follows_the_environment() {
    // The flag is read per call rather than once at startup, so it can be
    // switched from inside a running app. If it were cached, a test that ran
    // first would pin it for every test after.
    //
    // The guard is what makes the `remove_var` at the end safe: without it, this
    // test would clear the variable underneath a concurrent test that has just
    // set it and is asserting that the trace is on.
    let _guard = CaptureGuard::new();
    std::env::remove_var("NVRE_DRAG_TRACE");
    assert!(!trace(), "without NVRE_DRAG_TRACE the trace must be off");
    std::env::set_var("NVRE_DRAG_TRACE", "1");
    assert!(trace(), "NVRE_DRAG_TRACE=1 must enable the trace");
    std::env::remove_var("NVRE_DRAG_TRACE");
}

#[test]
fn the_drag_start_reaches_the_ghost() {
    // The end of the source half of the gesture: the message is written by the
    // panel, and this is the system that consumes it. The log line reports on
    // this transition, so this is the transition that has to happen for the
    // line to mean anything.
    //
    // The guard rather than a bare `set_var`: this test runs `begin_ghost`, which
    // logs, and a concurrent test that clears the flag underneath it would leave
    // this test's own assertion reading a trace that was turned off.
    let _guard = CaptureGuard::new();

    let mut app = App::new();
    app.add_message::<DragStarted>()
        .init_resource::<DragGhost>()
        .add_systems(Update, nvre_editor::object_drag::begin_ghost);
    app.world_mut().write_message(DragStarted(DraggedObject {
        form_id: 0x00AB_CDEF,
        sig: *b"STAT",
    }));
    app.update();

    assert_eq!(
        app.world().resource::<DragGhost>().object,
        Some(DraggedObject {
            form_id: 0x00AB_CDEF,
            sig: *b"STAT",
        }),
        "the drag start reaches the ghost"
    );
}

#[test]
fn the_prefix_is_the_one_the_log_is_grepped_for() {
    // The trace is only useful if its lines can be filtered out of a full
    // editor log. The prefix is asserted against the source so a rename is a
    // visible change rather than a silently empty grep.
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../nvre-editor/src/object_drag.rs"
    ))
    .expect("object_drag.rs");
    assert!(
        source.contains(&format!("\"{DRAG_PREFIX} {{}}\"")),
        "the trace must emit lines prefixed {DRAG_PREFIX:?}; found in: {}",
        source
            .lines()
            .find(|l| l.contains("[drag]"))
            .unwrap_or("(no [drag] line)")
    );
}

#[test]
fn every_drag_system_has_a_trace_line() {
    // The regression this guards against: a new early `return` added to a drag
    // system without a log line, which is exactly how a silent failure mode
    // gets introduced. Each system must carry at least one `trace_log!`.
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../nvre-editor/src/object_drag.rs"
    ))
    .expect("object_drag.rs");
    // Each system is the span from its `pub fn` to the next one.
    let mut spans: Vec<&str> = Vec::new();
    let mut rest = source.as_str();
    while let Some(at) = rest.find("pub fn ") {
        rest = &rest[at..];
        let end = rest[1..]
            .find("\npub fn ")
            .map(|e| e + 1)
            .unwrap_or(rest.len());
        spans.push(&rest[..end]);
        rest = &rest[end..];
    }
    for system in [
        "begin_ghost",
        "update_ghost",
        "place_dropped_object",
        "draw_ghost",
    ] {
        let span = spans
            .iter()
            .find(|s| s.contains(&format!("fn {system}(")))
            .unwrap_or_else(|| panic!("{system} not found in object_drag.rs"));
        assert!(
            span.contains("trace_log!"),
            "{system} has no trace line; an unlogged early return is invisible"
        );
    }
}

/// An app with the viewport window and camera the drag systems read.
///
/// `place_dropped_object` is not registered: it needs a `Session`, which means
/// the 465k-record master, and the trace under test is the ghost path — the drop
/// itself is covered by `tests/object_drop_system.rs`, which does load it.
fn app() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<DragGhost>()
        .add_message::<DragStarted>()
        .add_plugins(bevy::camera::CameraPlugin)
        .add_plugins(bevy::asset::AssetPlugin::default())
        .insert_resource(Assets::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>::default())
        .insert_resource(Assets::<bevy::gizmos::GizmoAsset>::default())
        .add_plugins(bevy::transform::TransformPlugin)
        // The gizmos plugin registers the store and the default group, and
        // `Gizmos`/`draw_ghost` read both. `MinimalPlugins` does not, so the
        // plugin is added here rather than hand-registering each piece.
        .add_plugins(bevy::gizmos::GizmoPlugin)
        .add_systems(Update, (begin_ghost, update_ghost, draw_ghost).chain());

    let window = app
        .world_mut()
        .spawn((
            Window {
                resolution: bevy::window::WindowResolution::new(1280, 800),
                ..default()
            },
            ViewportWindow,
        ))
        .id();
    app.world_mut().spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            far: 1_500_000.0,
            near: 1.0,
            ..default()
        }),
        Camera {
            computed: bevy::camera::ComputedCameraValues {
                clip_from_view: bevy::camera::Projection::Perspective(PerspectiveProjection {
                    far: 1_500_000.0,
                    near: 1.0,
                    ..default()
                })
                .get_clip_from_view(),
                target_info: Some(bevy::camera::RenderTargetInfo {
                    physical_size: UVec2::new(1280, 800),
                    scale_factor: 1.0,
                }),
                ..default()
            },
            ..default()
        },
        RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
        // Above the origin and looking *down* at it: a camera level with the
        // ground casts a ray parallel to it, and `ground_hit` rightly returns
        // `None` for that — so a level camera would make every test here
        // report "no landing point" for a reason that has nothing to do with
        // the drag.
        Transform::from_xyz(0.0, 250.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
        ViewportCamera,
    ));
    (app, window)
}

#[test]
fn a_drag_in_flight_logs_the_landing_point() {
    // The line the user's symptom turns on: during a drag the ghost either has
    // a landing point or it does not. This drives a drag through the real
    // systems and asserts the transition is reported.
    // Held for the whole test: the capture buffer is process-global, so without
    // this a concurrent test's lines land in the middle of this one's assertions.
    let _guard = CaptureGuard::new();
    install();

    let (mut app, window) = app();
    app.world_mut().write_message(DragStarted(DraggedObject {
        form_id: 0x00AB_CDEF,
        sig: *b"STAT",
    }));
    app.update();
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_physical_cursor_position(Some(DVec2::new(640.0, 400.0)));
    app.update();

    assert!(
        app.world().resource::<DragGhost>().is_showing(),
        "the ghost is showing during a drag"
    );
    let lines = drag_lines();
    assert!(
        lines.iter().any(|l| l.contains("landing point")),
        "a landing point is logged while the drag is in flight: {lines:?}"
    );
}

#[test]
fn an_idle_viewport_logs_nothing() {
    // The other half, and the one that makes the trace usable: an idle editor
    // must not emit drag lines at all, or the trace is worse than no trace.
    let _guard = CaptureGuard::new();
    install();

    let (mut app, _window) = app();
    app.update();
    app.update();

    assert!(
        drag_lines().is_empty(),
        "no drag line may be emitted while nothing is dragged: {:?}",
        drag_lines()
    );
}
