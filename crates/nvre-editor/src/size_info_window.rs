//! The Size Info window: how much of the file each record type occupies.
//!
//! Opened from the File Details window's top-right corner, over the same scanned
//! file — the breakdown is computed from the rows already in memory, so this
//! window never re-reads the plugin and never scans.
//!
//! It is a **separate window** rather than a tab or a panel inside File Details
//! because that is how GECK does it and because the two are read together: the
//! record list to find one record, the size list to find out what a type costs.

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy_egui::egui;

use crate::size_info::{self, SizeBreakdown};

/// The schedule label for the Size Info window.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SizeInfoSchedule;

/// Marker component for the Size Info window.
#[derive(Component)]
pub struct SizeInfoWindow;

/// Marker component for the Size Info window's camera.
#[derive(Component)]
pub struct SizeInfoCamera;

/// Whether the window is open, and what it is showing.
#[derive(Resource, Default)]
pub struct SizeInfoState {
    /// Whether the user asked for the window.
    pub open: bool,
    /// The breakdown to show, and the file it is of.
    ///
    /// Recomputed when the File Details window's file changes rather than every
    /// frame: the breakdown walks 542,000 rows, which is a click's worth of work
    /// and not a frame's.
    pub breakdown: Option<SizeBreakdown>,
    /// The file the breakdown describes, so a stale one is not shown for a new
    /// file.
    shown_for: Option<std::path::PathBuf>,
    /// Sort column: 0 type, 1 records, 2 data, 3 total. `None` means the default,
    /// which is total descending.
    pub sort: Option<usize>,
    /// Whether the sort is descending.
    pub sort_desc: bool,
}

impl SizeInfoState {
    /// Close the window.
    pub fn close(&mut self) {
        self.open = false;
        self.breakdown = None;
        self.shown_for = None;
    }

    /// Compute the breakdown for `details`, if it is not already the one showing.
    ///
    /// Called each frame by the window's own system, which is cheap when nothing
    /// has changed: the comparison is against the path.
    pub fn refresh(&mut self, details: &crate::file_details::FileDetails, file_size: u64) {
        if self.shown_for.as_deref() == Some(details.path.as_path()) {
            return;
        }
        self.breakdown = Some(size_info::breakdown(details, file_size));
        self.shown_for = Some(details.path.clone());
    }

    /// The types in the order to show them.
    pub fn ordered(&self) -> Vec<&size_info::TypeSize> {
        let Some(b) = &self.breakdown else {
            return Vec::new();
        };
        let mut rows: Vec<&size_info::TypeSize> = b.types.iter().collect();
        // The default is the breakdown's own order: largest total first.
        let Some(column) = self.sort else {
            return rows;
        };
        rows.sort_by(|a, b| {
            let ord = match column {
                0 => a.kind.cmp(&b.kind),
                1 => a.records.cmp(&b.records),
                2 => a.data.cmp(&b.data),
                _ => a.total.cmp(&b.total),
            };
            if self.sort_desc {
                ord.reverse()
            } else {
                ord
            }
        });
        rows
    }
}

/// Spawn or despawn the window.
pub fn manage_window(
    mut commands: Commands,
    mut state: ResMut<SizeInfoState>,
    details: Res<crate::file_details_window::FileDetailsState>,
    mut win_query: Query<(Entity, &mut Window), With<SizeInfoWindow>>,
    cam_query: Query<Entity, With<SizeInfoCamera>>,
) {
    // With no file open there is nothing to break down, and a window still
    // showing the last file's numbers would be describing something that is no
    // longer open. Closing File Details therefore closes this too.
    if details.wanted.is_none() {
        state.close();
    }
    let existing = win_query.iter_mut().next();
    let cam = cam_query.iter().next();

    match (state.open, existing) {
        (true, None) => {
            let window = commands
                .spawn((
                    Window {
                        title: "Size Info".into(),
                        // Narrower than File Details: four columns of numbers.
                        resolution: (760, 560).into(),
                        position: WindowPosition::new(IVec2::new(1320, 80)),
                        ..default()
                    },
                    SizeInfoWindow,
                ))
                .id();
            commands.spawn((
                Camera2d,
                Projection::Orthographic(bevy::camera::OrthographicProjection::default_2d()),
                bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
                bevy_egui::EguiSchedule::new(SizeInfoSchedule),
                // After the panels (0..=5), the Object Editor (6) and File
                // Details (7).
                crate::windows::PanelContext::<8>,
                SizeInfoCamera,
            ));
        }
        (false, Some((win_ent, _))) => {
            commands.entity(win_ent).despawn();
            if let Some(cam_ent) = cam {
                commands.entity(cam_ent).despawn();
            }
        }
        _ => {}
    }
}

/// Draw the window.
pub fn ui(
    mut ctx: Query<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<8>>>,
    mut state: ResMut<SizeInfoState>,
    details: Res<crate::file_details_window::FileDetailsState>,
) {
    let Some(mut ctx) = ctx.iter_mut().next() else {
        return;
    };
    let ctx = ctx.get_mut();

    // The breakdown is of the file File Details is showing. Nothing is computed
    // until that file has been scanned.
    if let Some(d) = &details.details {
        let size = std::fs::metadata(&d.path).map(|m| m.len()).unwrap_or(0);
        state.refresh(d, size);
    }

    egui::CentralPanel::default().show(&mut crate::panels::root_ui(ctx, "size_info"), |ui| {
        let Some(breakdown) = state.breakdown.clone() else {
            ui.label("No file open. Open one from the launcher's Details window.");
            return;
        };

        // The file and the total, which is the number every row is a share of.
        ui.horizontal(|ui| {
            if let Some(name) = breakdown
                .types
                .first()
                .map(|_| &breakdown)
                .and_then(|_| details.details.as_ref())
                .and_then(|d| d.path.file_name())
            {
                ui.strong(name.to_string_lossy().into_owned());
            }
            ui.separator();
            ui.label(format!(
                "{} in {} types",
                size_info::human_bytes(breakdown.file_size),
                breakdown.types.len()
            ));
        });
        // Every byte is attributed, so this should hold; saying so is what makes
        // the breakdown trustworthy rather than merely plausible.
        let accounted = breakdown.total();
        if accounted == breakdown.file_size {
            ui.weak(format!(
                "all {accounted} bytes accounted for ({} in the file header)",
                breakdown.header_bytes
            ));
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(0xd0, 0xa0, 0x40),
                format!(
                    "{accounted} of {} bytes accounted for — {} unaccounted",
                    breakdown.file_size,
                    breakdown.file_size as i64 - accounted as i64
                ),
            );
        }
        ui.separator();

        let mut sort = state.sort;
        let mut sort_desc = state.sort_desc;
        let rows = state.ordered();

        use egui_extras::{Column, TableBuilder};
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .id_salt("size_info_table")
            .column(Column::initial(90.0).at_least(70.0))
            .column(Column::initial(90.0).at_least(70.0))
            .column(Column::initial(110.0).at_least(80.0))
            .column(Column::initial(110.0).at_least(80.0))
            .column(Column::remainder().resizable(false).at_least(120.0))
            .header(20.0, |mut header| {
                // The headers are clickable: a breakdown's first question is
                // usually "by count or by size", and the default (size) is not
                // always it.
                for (i, name) in ["Type", "Records", "Data", "Total", "Share"]
                    .iter()
                    .enumerate()
                {
                    header.col(|ui| {
                        let arrow = if sort == Some(i) {
                            if sort_desc {
                                " ▼"
                            } else {
                                " ▲"
                            }
                        } else {
                            ""
                        };
                        if ui
                            .add(egui::Button::new(
                                egui::RichText::new(format!("{name}{arrow}")).strong(),
                            ))
                            .clicked()
                        {
                            if sort == Some(i) {
                                sort_desc = !sort_desc;
                            } else {
                                sort = Some(i);
                                // Sizes start largest-first; names start A-Z.
                                sort_desc = i != 0;
                            }
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(18.0, rows.len(), |mut row| {
                    let Some(t) = rows.get(row.index()) else {
                        return;
                    };
                    row.col(|ui| {
                        ui.monospace(&t.kind);
                    });
                    row.col(|ui| {
                        // A type with no records is a group label only — it costs
                        // its group headers and nothing else.
                        if t.records == 0 {
                            ui.weak("—");
                        } else {
                            ui.monospace(t.records.to_string());
                        }
                    });
                    row.col(|ui| {
                        ui.monospace(size_info::human_bytes(t.data));
                    });
                    row.col(|ui| {
                        ui.monospace(size_info::human_bytes(t.total));
                    });
                    row.col(|ui| {
                        // The share as a bar as well as a percentage: a bar
                        // compares two rows at a glance, which is the point of
                        // the window.
                        let frac = t.fraction(breakdown.file_size) as f32;
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(frac)
                                    .desired_width(90.0)
                                    .show_percentage(),
                            );
                        });
                    });
                });
            });

        if sort != state.sort || sort_desc != state.sort_desc {
            state.sort = sort;
            state.sort_desc = sort_desc;
        }
    });
}
