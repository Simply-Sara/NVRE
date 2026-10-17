//! The editor's panels, each drawn into its own window.
//!
//! GECK keeps its panels in separate movable windows; this does the same, with
//! each panel drawn by a system registered against that window's egui schedule
//! (see [`crate::windows`]). Every panel takes a `Single<&mut EguiContext, …>` so
//! it draws into its own window and nothing else — that is what keeps the
//! panels' widget state, focus and scroll position separate.
//!
//! Each panel's contents go in a [`egui::CentralPanel`], which is that panel's
//! Bevy window rather than an egui window nested inside it: the OS window is
//! already the frame, and a second one inside it only adds a title bar and a
//! drag handle.
//!
//! Immediate mode throughout: each frame reads [`crate::session::Session`] and
//! queues the changes it implies. Nothing here holds state across frames except
//! the filter strings and the worldspace dropdown, which are the only things a
//! user types into or picks.

use crate::session::CellScope;
use bevy::prelude::*;
use bevy_egui::egui;
use nvre_edit::encode::sub_views;

/// Filter text for the cell browser and the place palette.
///
/// The cell browser's worldspace dropdown lives here too: it is the other thing
/// the cell window's user types into or picks, and it has to survive the frames
/// in between for the list to stay filtered.
#[derive(Resource, Default)]
pub struct Filters {
    pub cell: String,
    pub place: String,
    /// Which cells the browser lists: the interiors, or one worldspace's.
    ///
    /// Defaults to [`CellScope::Interiors`], which is what GECK's Cell View
    /// opens with.
    pub cell_scope: CellScope,
}

/// The root [`egui::Ui`] covering a whole window.
///
/// Every panel draws into a [`egui::CentralPanel`], which *is* its Bevy window
/// rather than a window floating inside it. `CentralPanel::show` takes a
/// `&mut Ui` where `Window::show` took the context, so the root `Ui` is built
/// first — over the full viewport rect, on the background layer, which is what
/// `egui`'s own top-level pass does.
///
/// The id is per-panel: each panel has its own context anyway, but a distinct
/// id keeps the widget ids unambiguous. It is `&'static str` because an
/// [`egui::Id`] outlives the frame that made it.
pub fn root_ui(ctx: &egui::Context, id: &'static str) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        id.into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    )
}

/// The main window: the editor's front door.
///
/// The viewport and every panel own a window of their own, so this one holds only
/// what has no better home: where the data came from, and the actions that are
/// about the whole plugin rather than the open cell.
pub fn main_window(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<bevy_egui::PrimaryEguiContext>>,
    mut session: ResMut<crate::session::Session>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "main"), |ui| {
        ui.heading("New Vegas Rust Editor");
        ui.label(format!("plugin  {}", session.doc.name()));
        ui.label(format!("masters {:?}", session.masters));
        ui.label(format!("records {}", session.doc.records().len()));
        ui.separator();
        if ui.button("Save as .esp").clicked() {
            let out = session.save_target();
            match session.save(&out) {
                Ok(r) => {
                    session.status = format!(
                        "saved {} ({} records, {} bytes)",
                        out.display(),
                        r.records,
                        r.bytes
                    )
                }
                Err(e) => session.status = format!("save failed: {e}"),
            }
        }
        if ui.button("Reset selection").clicked() {
            session.select(0);
        }
        ui.separator();
        ui.label(&session.status);
    });
}

/// The cell browser: which cell is open, and what else there is.
///
/// The worldspace dropdown is the one GECK's Cell View leads with: it selects
/// either the interiors or one worldspace, and the list below it holds only that
/// scope's cells. It opens on the interiors, the way GECK does, so the browser
/// starts on interior cells rather than on 30,000 exterior ones.
pub fn cells(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<0>>>,
    mut session: ResMut<crate::session::Session>,
    mut filters: ResMut<Filters>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "cells"), |ui| {
        ui.heading("Cells");
        let worlds = session.worldspaces();
        let scope = filters.cell_scope;
        egui::ComboBox::from_id_salt("cell_scope")
            .selected_text(scope.label(&worlds))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                // Both write the scope straight into the resource, which is what
                // `selectable_value` is for: the list below reads it next frame.
                ui.selectable_value(&mut filters.cell_scope, CellScope::Interiors, "Interiors");
                for w in &worlds {
                    ui.selectable_value(
                        &mut filters.cell_scope,
                        CellScope::Worldspace(w.form_id),
                        &w.label,
                    );
                }
            });
        let filter = filters.cell.to_lowercase();
        let scope = filters.cell_scope;
        let cells = session.cells(scope);
        // Filtered here rather than in the session: the session's list is what
        // the dropdown counts, and the count has to say what the scope holds
        // rather than what the filter leaves.
        let rows: Vec<&crate::session::CellRow> = cells
            .iter()
            .filter(|c| {
                filter.is_empty()
                    || c.editor_id().to_lowercase().contains(&filter)
                    || c.name
                        .as_deref()
                        .is_some_and(|n| n.to_lowercase().contains(&filter))
            })
            .collect();
        let shown = cells.len();
        let matched = rows.len();
        // A header row, then one row per cell, both in the same grid so the
        // columns line up. Clicking a row opens that cell, as the list did.
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("cells_table")
                .num_columns(4)
                .striped(true)
                .min_col_width(60.0)
                .show(ui, |ui| {
                    ui.strong("Editor ID");
                    ui.strong("Form ID");
                    ui.strong("Name");
                    ui.strong("Location");
                    ui.end_row();
                    for c in &rows {
                        let open = session.cell == Some(c.form_id);
                        // The Editor ID cell is the clickable one, so a row is
                        // opened the way the old list was: click the name.
                        if ui
                            .selectable_label(open, c.editor_id())
                            .on_hover_text(format!("open {} ({} refs)", c.editor_id(), c.refs))
                            .clicked()
                        {
                            session.open_cell(c.form_id);
                        }
                        // The fixed-width columns are monospace, so the Form IDs
                        // and the grid coordinates line up down the column.
                        ui.label(egui::RichText::new(format!("{:08X}", c.form_id)).monospace());
                        ui.label(c.name.as_deref().unwrap_or(""));
                        ui.label(egui::RichText::new(c.location()).monospace());
                        ui.end_row();
                    }
                });
        });
        ui.separator();
        ui.label(format!(
            "{shown} cells in {}{}",
            scope.label(&worlds),
            if matched == shown {
                String::new()
            } else {
                format!(" ({matched} match)")
            }
        ));
        ui.label("filter");
        ui.text_edit_singleline(&mut filters.cell);
    });
}

/// The references in the open cell, and the delete action.
pub fn references(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<1>>>,
    mut session: ResMut<crate::session::Session>,
    mut clicks: Local<crate::viewport::ClickState>,
    mut frame: MessageWriter<crate::FrameRequested>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "references"), |ui| {
        ui.heading("References");
        let Some(cell) = session.cell else {
            ui.label("no cell open");
            return;
        };
        let name = session
            .index
            .cells
            .get(&cell)
            .and_then(|c| c.edid.clone())
            .unwrap_or_else(|| format!("{cell:08X}"));
        let count = session.cell_refs().len();
        ui.label(format!("{name}: {count} refs"));
        ui.separator();
        let queued = session.deleted().to_vec();
        // The list is read out before the loop, which needs `session` mutably
        // to change the selection.
        let refs = session.cell_refs().to_vec();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for r in &refs {
                let label = r
                    .edid
                    .clone()
                    .unwrap_or_else(|| format!("{} {:08X}", r.sig, r.form_id));
                // A persistent reference is marked with a star, the way a mod
                // list marks a master.
                let mut text = if r.persistent {
                    format!("{label} *")
                } else {
                    label
                };
                if queued.contains(&r.form_id) {
                    text = format!("{text}  [deleting]");
                }
                if ui
                    .selectable_label(session.selected == Some(r.form_id), text)
                    .clicked()
                {
                    session.select(r.form_id);
                    // A double-click on a list entry frames it in the viewport,
                    // the same as double-clicking the model itself. The click
                    // state is this panel's own, so a double-click in the
                    // viewport and one here do not interfere.
                    if clicks.register(r.form_id) {
                        frame.write(crate::FrameRequested { form_id: r.form_id });
                    }
                }
            }
        });
        ui.separator();
        if ui.button("Delete selected").clicked() {
            session.delete_selected();
        }
    });
}

/// The property grid for the selected reference.
pub fn properties(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<2>>>,
    mut session: ResMut<crate::session::Session>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "properties"), |ui| {
        let Some(fid) = session.selected else {
            ui.label("nothing selected");
            return;
        };
        // Everything the grid shows is read out before the mutable calls below,
        // so the record's borrow ends here.
        let Some(rec) = session.selected_record() else {
            ui.label(format!("{fid:08X} not found"));
            return;
        };
        let rec = rec.record();
        let sig_label = rec.sig_str();
        let pos = crate::session::position_of(rec);
        let views = sub_views(&session.schema, rec).unwrap_or_default();
        ui.label(format!("{:08X} {}", fid, sig_label));
        ui.separator();
        // Move the selection in game units. The viewport's gizmo will drive the
        // same call once it can drag.
        if let Some(p) = pos {
            ui.label(format!("position {:.1}, {:.1}, {:.1}", p[0], p[1], p[2]));
            ui.horizontal(|ui| {
                for (label, d) in [
                    ("X-", [-10.0, 0.0, 0.0]),
                    ("X+", [10.0, 0.0, 0.0]),
                    ("Y-", [0.0, -10.0, 0.0]),
                    ("Y+", [0.0, 10.0, 0.0]),
                    ("Z-", [0.0, 0.0, -10.0]),
                    ("Z+", [0.0, 0.0, 10.0]),
                ] {
                    if ui.small_button(label).clicked() {
                        session.nudge_selected(d);
                    }
                }
            });
            ui.separator();
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for s in views {
                ui.horizontal(|ui| {
                    ui.label(format!("{} ({})", s.name, s.sig));
                    if !s.writable {
                        ui.colored_label(egui::Color32::YELLOW, "read-only");
                    }
                });
                let mut text = serde_json::to_string_pretty(&s.value).unwrap_or_default();
                ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                if let Some(p) = s.problem {
                    ui.colored_label(egui::Color32::RED, p);
                }
            }
        });
    });
}

/// The place palette: pick a base object and put it in the open cell.
pub fn place(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<3>>>,
    mut session: ResMut<crate::session::Session>,
    mut filters: ResMut<Filters>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "place"), |ui| {
        ui.label("Base object");
        ui.text_edit_singleline(&mut filters.place);
        ui.separator();
        // The palette is cached in the session and rebuilt only when the filter
        // changes: the query walks the load order, which costs hundreds of
        // milliseconds, and the panel draws every frame.
        let objects = session.palette(&filters.place);
        let shown = objects.len();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (fid, label) in objects {
                if ui.button(&label).clicked() {
                    // Placed at the cell's origin: the camera's target is the
                    // better default once the viewport can report it.
                    session.place(fid, [0.0, 0.0, 0.0]);
                }
            }
            if shown == 200 && filters.place.is_empty() {
                ui.label("showing 200 of 11483 — type to filter");
            }
        });
    });
}

/// The status line: what the last action did, and how much is unsaved.
pub fn status(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<4>>>,
    mut session: ResMut<crate::session::Session>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "status"), |ui| {
        ui.horizontal(|ui| {
            ui.label(&session.status);
            ui.separator();
            let pending = session.pending_edits();
            if pending > 0 {
                ui.colored_label(egui::Color32::YELLOW, format!("{pending} unsaved"));
            } else {
                ui.label("saved");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Save as .esp").clicked() {
                    // Next to the plugin being edited, with a `.esp` name.
                    let out = session.save_target();
                    match session.save(&out) {
                        Ok(r) => {
                            session.status = format!(
                                "saved {} ({} records, {} bytes)",
                                out.display(),
                                r.records,
                                r.bytes
                            );
                        }
                        Err(e) => session.status = format!("save failed: {e}"),
                    }
                }
            });
        });
    });
}

/// The debug window: the camera's state, read and written.
///
/// The orbit camera is the one thing in the editor whose state is worth watching
/// while it moves: a viewport that shows nothing is usually the camera looking
/// somewhere else, and the numbers say where. Editing them here moves the camera
/// the same way dragging does, which makes a framing problem something you can
/// dial in rather than hunt for.
pub fn debug(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<5>>>,
    mut orbit: ResMut<crate::viewport::Orbit>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "debug"), |ui| {
        ui.heading("Camera");
        egui::Grid::new("orbit").num_columns(2).show(ui, |ui| {
            // The target is where the camera looks, in Bevy space. A cell sits
            // tens of thousands of units from the origin, so the values are
            // shown to a unit rather than the default two decimals.
            let mut target = [orbit.target.x, orbit.target.y, orbit.target.z];
            ui.label("target");
            ui.horizontal(|ui| {
                for (i, axis) in ["x", "y", "z"].iter().enumerate() {
                    ui.label(*axis);
                    ui.add(egui::DragValue::new(&mut target[i]).speed(10.0));
                }
            });
            ui.end_row();
            if target != [orbit.target.x, orbit.target.y, orbit.target.z] {
                orbit.target = Vec3::new(target[0], target[1], target[2]);
            }

            ui.label("distance");
            ui.add(
                egui::DragValue::new(&mut orbit.distance)
                    .speed(10.0)
                    .range(20.0..=200_000.0),
            );
            ui.end_row();

            // Angles in degrees, which is what a person reads; stored in radians.
            let mut yaw = orbit.yaw.to_degrees();
            let mut pitch = orbit.pitch.to_degrees();
            ui.label("yaw");
            ui.add(egui::DragValue::new(&mut yaw).speed(1.0).suffix("°"));
            ui.end_row();
            ui.label("pitch");
            ui.add(
                egui::DragValue::new(&mut pitch)
                    .speed(1.0)
                    .range(-88.0..=88.0)
                    .suffix("°"),
            );
            ui.end_row();
            orbit.yaw = yaw.to_radians();
            orbit.pitch = pitch.to_radians();
        });

        ui.separator();
        // Where the camera ends up, which is what the target and angles imply.
        // Dragging the target above moves this; reading it says whether the
        // camera is where you think it is.
        let dir = Vec3::new(
            orbit.yaw.cos() * orbit.pitch.cos(),
            orbit.pitch.sin(),
            orbit.yaw.sin() * orbit.pitch.cos(),
        );
        let eye = orbit.target - dir * orbit.distance;
        ui.label(format!("eye  {:.0}, {:.0}, {:.0}", eye.x, eye.y, eye.z));
        ui.label(format!(
            "yaw {:.1}°  pitch {:.1}°",
            orbit.yaw.to_degrees(),
            orbit.pitch.to_degrees()
        ));
        ui.label(format!("distance {:.0}", orbit.distance));

        ui.separator();
        if ui.button("Reset").clicked() {
            *orbit = crate::viewport::Orbit::default();
        }
    });
}
