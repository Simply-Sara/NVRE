//! The File Details window: every record in a plugin, one row each.
//!
//! The scan and the data live in [`crate::file_details`]; this is the window
//! around them, and it follows the Object Editor's pattern — spawned when the
//! launcher asks for it, despawned when it is closed, in its own Bevy window with
//! its own egui context.
//!
//! # Why the scan runs off the frame
//!
//! `FalloutNV.esm` is 465,016 records and 562 ms of scanning [measured]; a system
//! that draws runs on the main thread, so scanning there would freeze the window
//! for that long and every frame after it. The scan therefore runs on the
//! [`AsyncComputeTaskPool`] and the window shows its state while it runs.
//!
//! # Why the table is virtualized
//!
//! `egui_extras`' `body.rows` only asks for the rows that are on screen, which is
//! the only way a 1.5-million-row table (Fallout 4) is drawable at all. The
//! columns are packed arrays, so a visible row is a few array reads rather than a
//! struct walk.

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::tasks::{block_on, poll_once, AsyncComputeTaskPool, Task};
use bevy_egui::egui;
use std::path::PathBuf;

use crate::file_details::{self, FileDetails, RowKind};

/// The schedule label for the File Details window.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileDetailsSchedule;

/// Marker component for the File Details window.
#[derive(Component)]
pub struct FileDetailsWindow;

/// Marker component for the File Details window's camera.
#[derive(Component)]
pub struct FileDetailsCamera;

/// What the window is doing.
#[derive(Resource, Default)]
pub struct FileDetailsState {
    /// The file to show, set by the launcher. `None` closes the window.
    pub wanted: Option<PathBuf>,
    /// The scan in flight, while there is one.
    task: Option<Task<Result<FileDetails, file_details::DetailsError>>>,
    /// What the scan is of, so a finished task is known to match `wanted`.
    scanning: Option<PathBuf>,
    /// The rows, once the scan has finished.
    pub details: Option<FileDetails>,
    /// Why the last scan failed.
    pub error: Option<String>,
    /// The row the user last clicked, as an index into the table.
    pub selected: Option<usize>,
    /// The filter text. A record's type, Editor ID and FormID are all searched,
    /// because those are the three ways GECK's own dialog is used.
    pub filter: String,
    /// Rows passing the current filter, as indices into `details`.
    ///
    /// Empty when the filter is empty, which means "every row" — building a
    /// 465,000-entry identity vector for an empty filter would be a cost with no
    /// purpose.
    visible: Vec<u32>,
}

impl FileDetailsState {
    /// Open `path`, replacing whatever was showing.
    ///
    /// The scan is started here rather than in a system so the launcher can ask
    /// for it in the same frame as the click.
    pub fn open(&mut self, path: PathBuf) {
        if self.wanted.as_deref() == Some(path.as_path()) {
            // Already showing this file: keep the rows and the scroll position
            // rather than rescanning for a second click on the same button.
            return;
        }
        self.wanted = Some(path.clone());
        self.details = None;
        self.error = None;
        self.selected = None;
        self.filter.clear();
        self.visible.clear();
        self.scanning = Some(path.clone());
        let pool = AsyncComputeTaskPool::get();
        self.task = Some(pool.spawn(async move { file_details::scan(&path) }));
    }

    /// Close the window.
    pub fn close(&mut self) {
        self.wanted = None;
        self.task = None;
        self.scanning = None;
        self.details = None;
        self.error = None;
        self.selected = None;
        self.filter.clear();
        self.visible.clear();
    }

    /// Whether a scan is running.
    pub fn is_scanning(&self) -> bool {
        self.scanning.is_some()
    }

    /// How many rows the table shows: the filtered count, or every row.
    pub fn row_count(&self) -> usize {
        match &self.details {
            Some(d) if self.filter.is_empty() => d.len(),
            Some(_) => self.visible.len(),
            None => 0,
        }
    }

    /// The row index in `details` for table row `row`.
    pub fn row_at(&self, row: usize) -> Option<usize> {
        if self.filter.is_empty() {
            return Some(row);
        }
        self.visible.get(row).map(|i| *i as usize)
    }

    /// Recompute which rows pass the filter.
    ///
    /// Called when the filter changes rather than every frame: 465,000 string
    /// comparisons per frame is not a draw cost anyone can pay.
    pub fn refilter(&mut self) {
        self.visible.clear();
        let Some(d) = &self.details else { return };
        if self.filter.is_empty() {
            return;
        }
        let needle = self.filter.to_lowercase();
        for row in 0..d.len() {
            let kind = d
                .kind
                .get(row)
                .map(|k| String::from_utf8_lossy(&k).into_owned())
                .unwrap_or_default();
            let id = d.editor_id.get(row).unwrap_or("");
            let fid = d
                .form_id
                .get(row)
                .map(|f| format!("{f:08X}"))
                .unwrap_or_default();
            if kind.to_lowercase().contains(&needle)
                || id.to_lowercase().contains(&needle)
                || fid.to_lowercase().contains(&needle)
            {
                self.visible.push(row as u32);
            }
        }
    }
}

/// Poll the running scan, and take its result.
///
/// Runs in `Update` so the rows appear the frame after the scan finishes, rather
/// than only when the window is redrawn.
pub fn poll_scan(mut state: ResMut<FileDetailsState>) {
    let Some(task) = state.task.as_mut() else {
        return;
    };
    let Some(result) = block_on(poll_once(task)) else {
        return;
    };
    state.task = None;
    state.scanning = None;
    match result {
        Ok(details) => {
            // A scan that finished after the user asked for another file is
            // stale: `wanted` no longer names it, so its rows would be the wrong
            // file's.
            if state.wanted.as_deref() == Some(details.path.as_path()) {
                state.details = Some(details);
            }
        }
        Err(e) => state.error = Some(e.to_string()),
    }
}

/// Spawn or despawn the window, following the launcher's request.
pub fn manage_window(
    mut commands: Commands,
    state: Res<FileDetailsState>,
    mut win_query: Query<(Entity, &mut Window), With<FileDetailsWindow>>,
    cam_query: Query<Entity, With<FileDetailsCamera>>,
) {
    let existing = win_query.iter_mut().next();
    let cam = cam_query.iter().next();

    match (&state.wanted, existing) {
        (Some(path), Some((_, mut win))) => {
            let title = format!(
                "File Details — {}",
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            );
            if win.title != title {
                win.title = title;
            }
        }
        (Some(path), None) => {
            let title = format!(
                "File Details — {}",
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            );
            let window = commands
                .spawn((
                    Window {
                        title,
                        // Wide: seven columns, one of them an Editor ID.
                        resolution: (1180, 720).into(),
                        position: WindowPosition::new(IVec2::new(120, 80)),
                        ..default()
                    },
                    FileDetailsWindow,
                ))
                .id();
            commands.spawn((
                Camera2d,
                Projection::Orthographic(bevy::camera::OrthographicProjection::default_2d()),
                bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
                bevy_egui::EguiSchedule::new(FileDetailsSchedule),
                // The next free context marker: the panels use 0..=5 and the
                // Object Editor 6.
                crate::windows::PanelContext::<7>,
                FileDetailsCamera,
            ));
        }
        (None, Some((win_ent, _))) => {
            commands.entity(win_ent).despawn();
            if let Some(cam_ent) = cam {
                commands.entity(cam_ent).despawn();
            }
        }
        (None, None) => {}
    }
}

/// Draw the window.
pub fn ui(
    mut ctx: Query<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<7>>>,
    mut state: ResMut<FileDetailsState>,
) {
    let Some(mut ctx) = ctx.iter_mut().next() else {
        return;
    };
    let ctx = ctx.get_mut();

    egui::CentralPanel::default().show(&mut crate::panels::root_ui(ctx, "file_details"), |ui| {
        // The filter and the count above the table: GECK's dialog has a record
        // count, and a 465,000-row table needs a way to find a row.
        ui.horizontal(|ui| {
            ui.label("Filter:");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut state.filter)
                    .desired_width(260.0)
                    .hint_text("type, editor id or form id"),
            );
            if resp.changed() {
                state.refilter();
            }
            if !state.filter.is_empty() && ui.button("Clear").clicked() {
                state.filter.clear();
                state.refilter();
            }
            ui.separator();
            if state.is_scanning() {
                ui.spinner();
                ui.label("reading…");
            } else if let Some(err) = &state.error {
                ui.colored_label(egui::Color32::RED, err);
            } else {
                let (rows, records) = state
                    .details
                    .as_ref()
                    .map(|d| (d.len(), d.record_count()))
                    .unwrap_or((0, 0));
                let shown = state.row_count();
                // Both numbers, because the list holds groups and the header as
                // well as records: "465,016 records" over a list of 470,000 rows
                // would look like a bug.
                if shown == rows {
                    ui.label(format!("{records} records in {rows} rows"));
                } else {
                    ui.label(format!("{shown} of {rows} rows ({records} records)"));
                }
            }
        });
        ui.separator();

        if state.details.is_none() {
            if !state.is_scanning() && state.error.is_none() {
                ui.label("No file open.");
            }
            return;
        }

        let rows = state.row_count();
        let selected = state.selected;
        // The clicked row, applied after the table's closure: the closure borrows
        // `state` immutably to read `details`, so writing `selected` inside it is
        // an overlapping borrow.
        let mut clicked: Option<usize> = None;
        // Read out of `state` before the closure takes its borrow, so the closure
        // needs nothing from `state` at all.
        let filter_is_empty = state.filter.is_empty();
        let visible: &[u32] = &state.visible;
        let details = state.details.as_ref().unwrap();

        use egui_extras::{Column, TableBuilder};
        TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .resizable(true)
            .id_salt("file_details_table")
            .column(Column::initial(56.0).at_least(48.0))
            .column(Column::initial(64.0).at_least(56.0))
            .column(Column::initial(64.0).at_least(56.0))
            .column(Column::initial(300.0).at_least(120.0).clip(true))
            .column(Column::initial(96.0).at_least(88.0))
            .column(Column::initial(96.0).at_least(88.0))
            .column(Column::initial(72.0).at_least(64.0))
            .column(Column::remainder().resizable(false).at_least(64.0))
            .header(20.0, |mut header| {
                for name in [
                    "Type",
                    "Deleted",
                    "Ignored",
                    "Editor ID",
                    "Form ID",
                    "Offset",
                    "Length",
                    "VC",
                ] {
                    header.col(|ui| {
                        ui.strong(name);
                    });
                }
            })
            .body(|body| {
                body.rows(18.0, rows, |mut row| {
                    let table_row = row.index();
                    let index = if filter_is_empty {
                        table_row
                    } else {
                        match visible.get(table_row) {
                            Some(i) => *i as usize,
                            None => return,
                        }
                    };
                    row.set_selected(selected == Some(index));

                    // The type, indented by the row's depth so the group tree
                    // reads as a tree. A `GRUP` is shown as its label (which is
                    // in the Editor ID column) plus its type, and the `TES4`
                    // root is marked, because those rows are structure rather
                    // than content.
                    let depth = details.depth.get(index).unwrap_or(0) as usize;
                    let kind = details.row_kind.get(index).unwrap_or_default();
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(depth as f32 * 10.0);
                            if let Some(k) = details.kind.get(index) {
                                let text = String::from_utf8_lossy(&k).into_owned();
                                match kind {
                                    RowKind::Record => {
                                        ui.monospace(text);
                                    }
                                    // Weaker, so a scan down the Type column finds
                                    // records and not the scaffolding.
                                    RowKind::Group | RowKind::Header => {
                                        ui.weak(egui::RichText::new(text).monospace());
                                    }
                                }
                            }
                        });
                    });
                    row.col(|ui| {
                        if details.deleted.get(index).unwrap_or(false) {
                            ui.label("●");
                        }
                    });
                    row.col(|ui| {
                        if details.ignored.get(index).unwrap_or(false) {
                            ui.label("●");
                        }
                    });
                    row.col(|ui| {
                        match details.editor_id.get(index) {
                            Some(id) => ui.label(id),
                            // GECK leaves the cell empty for a record with no
                            // EDID, which is most of them.
                            None => ui.label(""),
                        };
                    });
                    // A group header has no FormID and no revision word, so
                    // showing a zero there would be a number that means nothing.
                    // Its offset and size are real, though, so those stay.
                    let is_group = kind == RowKind::Group;
                    row.col(|ui| {
                        if is_group {
                            return;
                        }
                        if let Some(fid) = details.form_id.get(index) {
                            ui.monospace(format!("{fid:08X}"));
                        }
                    });
                    row.col(|ui| {
                        // Decimal, not hex: the offset is a byte position in the
                        // file, and a reader comparing it against a hex dump is
                        // the rarer case than one reading it as a number.
                        if let Some(off) = details.offset.get(index) {
                            ui.monospace(off.to_string());
                        }
                    });
                    row.col(|ui| {
                        if let Some(len) = details.length.get(index) {
                            ui.monospace(len.to_string());
                        }
                    });
                    row.col(|ui| {
                        if is_group {
                            return;
                        }
                        if let Some(vc) = details.vc.get(index) {
                            ui.monospace(format!("{vc:08X}"));
                        }
                    });

                    if row.response().clicked() {
                        clicked = Some(index);
                    }
                });
            });
        // After the table's borrow of `details` has ended.
        if let Some(index) = clicked {
            state.selected = Some(index);
        }
    });
}
