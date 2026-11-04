//! Where a panel's vertical scroll bar lands.
//!
//! The bar used to be painted at the right edge of the *content* rather than of
//! the panel: egui's `ScrollArea` defaults to `auto_shrink` on both axes, so a
//! grid of fields narrower than its window shrank the scroll area to the grid's
//! own width and the bar sat mid-panel. Worse, it moved — a record with a longer
//! value widened the grid's column and dragged the bar with it, which is what
//! made the position look arbitrary.
//!
//! These tests render a panel the way the editor does and read the bar's own
//! interact rect back out of the context, so the invariant is checked against
//! egui's layout rather than against a constant.

use nvre_editor::egui;
use std::sync::atomic::{AtomicUsize, Ordering};

/// What a rendered panel says about its scroll bar.
struct Rendered {
    /// The right edge of the panel the scroll area was drawn into.
    panel_right: f32,
    /// The right edge of the bar's own interact rect.
    bar_right: f32,
}

/// Render a panel of `panel_width` whose scroll area holds content `content_width`
/// wide, and report where the bar ended up.
///
/// The scroll-area id has to be unique per render because egui keys the whole
/// scroll state — including where the bar is and how animated-in it is — off it:
/// two renders under one id would share the first one's bar. Two passes are run
/// because the bar animates in over a frame and its rect is only settled once it
/// is fully shown.
fn render(content_width: f32, scroll_area: impl Fn(&str) -> egui::ScrollArea) -> Rendered {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let salt = format!("bar_probe_{}", NEXT.fetch_add(1, Ordering::Relaxed));

    let ctx = egui::Context::default();
    let panel_width = 800.0;
    let panel_height = 600.0;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(panel_width, panel_height),
        )),
        ..Default::default()
    };

    let mut area_id = None;
    for _ in 0..3 {
        let mut id = None;
        let mut out = ctx.run_ui(input.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let shown = scroll_area(&salt).show(ui, |ui| {
                    // A fixed content width, so the test says what the content's
                    // size is rather than letting a label decide it.
                    ui.set_width(content_width);
                    for i in 0..200 {
                        ui.label(format!("row {i}"));
                    }
                });
                id = Some(shown.id);
            });
        });
        // egui panics when a `TexturesDelta` with unapplied deltas is dropped, and
        // there is no renderer here to apply them.
        out.textures_delta.clear();
        area_id = id;
    }

    let bar = ctx
        .read_response(area_id.expect("the scroll area drew").with(1))
        .expect("the vertical bar has an interact rect");
    Rendered {
        panel_right: panel_width,
        bar_right: bar.rect.max.x,
    }
}

#[test]
fn the_bar_sits_at_the_panels_right_edge() {
    let r = render(200.0, nvre_editor::panels::vertical_scroll);
    // Within the panel's own margin, rather than at 200-and-something where the
    // content ends.
    assert!(
        r.panel_right - r.bar_right < 24.0,
        "the bar's right edge is {} against a panel edge of {} — it is following \
         the content, not the panel",
        r.bar_right,
        r.panel_right
    );
}

#[test]
fn the_bar_does_not_move_when_the_content_widens() {
    // The symptom that made the bar look arbitrary: a record with a longer value
    // widened the grid, and the bar moved with it.
    let narrow = render(200.0, nvre_editor::panels::vertical_scroll);
    let wide = render(700.0, nvre_editor::panels::vertical_scroll);
    assert_eq!(
        narrow.bar_right, wide.bar_right,
        "the bar moved from {} to {} as the content went from 200 to 700 wide",
        narrow.bar_right, wide.bar_right
    );
}

#[test]
fn content_wider_than_the_panel_keeps_the_bar_at_the_visible_edge() {
    // The other half of the same symptom: a record whose grid is wider than the
    // window used to push the bar off the edge. The bar belongs against the
    // visible edge, where the clip rect is — not out at the content's width.
    let overflowing = render(1200.0, nvre_editor::panels::vertical_scroll);
    assert!(
        overflowing.panel_right - overflowing.bar_right < 24.0,
        "with 1200px of content in an 800px panel the bar is at {}, past the \
         panel edge of {}",
        overflowing.bar_right,
        overflowing.panel_right
    );
}

#[test]
fn a_default_scroll_area_is_the_thing_this_guards_against() {
    // The helper exists because egui's own default is the behaviour the editor
    // had. If this ever stops holding, `vertical_scroll` is fixing nothing and
    // the test above is passing for the wrong reason.
    let default = render(200.0, |salt| egui::ScrollArea::vertical().id_salt(salt));
    assert!(
        default.panel_right - default.bar_right > 400.0,
        "egui's default no longer shrinks to the content (bar at {}); \
         `vertical_scroll`'s `auto_shrink` may now be a no-op",
        default.bar_right
    );
}
