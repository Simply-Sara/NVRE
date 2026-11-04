//! The object bounds scroll with the page they belong to.
//!
//! OBND used to be drawn by the object editor rather than by any page, because
//! nearly every object carries it — and it was drawn *after* the scroll area
//! closed: a band under a body that scrolls. On a record with enough fields to
//! fill the window that put the bounds below the bottom edge, drawn but
//! unreachable — the page scrolled and they did not come with it. On a short
//! record it looked fine, which is why the bug survived.
//!
//! The test that discriminates is the scroll: wheel the page down and the bounds
//! have to move. A band drawn after the scroll area does not move, ever.
//!
//! The bounds are on the Model List tab now, beside the model path and the
//! model's other subrecords, so they are inside the scroll area by construction
//! and move with it. These tests still check that, because "inside the scroll
//! area by construction" is exactly the kind of claim that stops being true the
//! next time the page is rearranged — and because the window still reports where
//! they landed, for the same reason it did before.
//!
//! Needs `NVRE_DATA` pointing at the game's Data folder; without it the tests
//! skip, like the other session-backed ones.

use nvre_editor::egui;
use nvre_editor::object_editor::{self, BodyLayout, ObjectEditorState, Pages};
use nvre_editor::session::Session;

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

fn schema_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")
}

/// A session with one record of `sig` loaded into the object editor's state.
fn open(sig: [u8; 4]) -> Option<(Session, ObjectEditorState)> {
    let data = data_dir()?;
    let mut session = Session::open(
        &std::path::Path::new(&data).join("FalloutNV.esm"),
        &schema_dir(),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()?;
    let fid = session.records_of(sig).first()?.0;
    session.open_object(fid, sig);
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid, sig);
    assert!(
        state.raw_subs.contains_key(b"OBND"),
        "{} carries no OBND, so the bounds are not drawn for it",
        String::from_utf8_lossy(&sig)
    );
    Some((session, state))
}

/// Render the body into a panel of `size`, wheeling it down by `wheel` points.
///
/// The pointer matters: a scroll area only takes the wheel when the pointer is
/// over it, so a wheel with no pointer moves nothing and the test would pass
/// against any placement at all.
fn render(sig: [u8; 4], size: egui::Vec2, wheel: f32) -> Option<BodyLayout> {
    let (mut session, mut state) = open(sig)?;
    let mut pages = Pages::default();

    let ctx = egui::Context::default();
    let pointer = egui::Pos2::new(size.x / 2.0, size.y / 2.0);
    let base = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..Default::default()
    };

    let mut layout = None;
    // Several passes: the bar's width settles after the first, and the offset a
    // wheel asks for is applied on the pass after the one that asked.
    for pass in 0..6 {
        let mut frame = base.clone();
        frame.events.push(egui::Event::PointerMoved(pointer));
        // One notch, on one pass. A wheel that kept arriving would keep
        // scrolling, and the test wants a single step it can measure.
        if wheel != 0.0 && pass == 2 {
            frame.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, wheel),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            });
        }
        let mut out = ctx.run_ui(frame, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                state.tab.chosen = Some("ModelList".to_string());
                layout = Some(object_editor::body(
                    ui,
                    &mut session,
                    &mut state,
                    &mut pages,
                ));
            });
        });
        // egui panics if a `TexturesDelta` is dropped with deltas unapplied.
        out.textures_delta.clear();
    }
    layout
}

#[test]
fn the_bounds_reach_the_visible_area_by_scrolling() {
    // The bounds are on the Model List tab, which is not the tab the record opens
    // on, so they are below the fold to begin with. What matters is that the page
    // can reach them: wheel all the way down and they have to be on screen.
    //
    // This is the case that used to be invisible. The bounds were drawn under the
    // scroll area, so they were on screen at the top of a short record and
    // unreachable on a long one — the bug only showed where the page was tall, and
    // a short record is the one a test writes first.
    let size = egui::vec2(700.0, 2000.0);
    let Some(layout) = render(*b"CREA", size, -100_000.0) else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let bounds = layout.bounds.expect("the bounds were drawn");
    assert!(
        layout.visible.intersects(bounds),
        "scrolled to the end, the bounds at {bounds:?} still do not intersect the \
         visible area {:?} — the bar cannot bring them into view",
        layout.visible
    );
}

#[test]
fn the_wheel_moves_the_bounds_with_the_page() {
    // The fix, stated as the behaviour a user sees. Scroll down and the bounds
    // come up; a band drawn after the scroll area stays exactly where it was.
    let size = egui::vec2(700.0, 300.0);
    let Some(at_top) = render(*b"CREA", size, 0.0).and_then(|l| l.bounds) else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let Some(scrolled) = render(*b"CREA", size, -300.0).and_then(|l| l.bounds) else {
        panic!("the scrolled render did not report the bounds");
    };
    assert!(
        scrolled.min.y < at_top.min.y,
        "the bounds did not move when the page scrolled: {} at the top, {} after \
         wheeling down. They are outside the scroll area, which is what pinned \
         them under the page",
        at_top.min.y,
        scrolled.min.y
    );
}

#[test]
fn the_bounds_come_into_view_when_the_page_scrolls_to_its_end() {
    // What "reachable" means: a panel too short for the page must still be able
    // to show the bounds, by scrolling all the way down.
    let size = egui::vec2(700.0, 300.0);
    let Some(layout) = render(*b"CREA", size, -100_000.0) else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let bounds = layout.bounds.expect("the bounds were drawn");
    assert!(
        layout.visible.intersects(bounds),
        "scrolled to the end, the bounds at {bounds:?} still do not intersect the \
         visible area {:?} — the bar cannot bring them into view",
        layout.visible
    );
}
