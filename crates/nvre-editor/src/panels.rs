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

/// Filter text for the cell browser, the place palette and the references table.
///
/// The cell browser's worldspace dropdown lives here too: it is the other thing
/// the cell window's user types into or picks, and it has to survive the frames
/// in between for the list to stay filtered.
#[derive(Resource)]
pub struct Filters {
    pub cell: String,
    pub place: String,
    /// Text the references table matches against a row's Editor ID, FormID,
    /// base object and owner.
    pub reference: String,
    /// Which record types the references table lists, by base object.
    ///
    /// `None` shows every type, the way GECK's reference list does; a chosen
    /// signature narrows the cell to that kind of object, which is how a cell's
    /// 155 references become the four doors in it.
    pub reference_type: Option<String>,
    /// Which cells the browser lists: the interiors, or one worldspace's.
    ///
    /// Defaults to [`CellScope::Interiors`], which is what GECK's Cell View
    /// opens with.
    pub cell_scope: CellScope,
    /// How many neighboring exterior cells around the active cell to render in the viewport.
    /// 0 = 1x1 cell, 1 = 3x3, 2 = 5x5 zone (default, like GECK), 3 = 7x7.
    pub exterior_radius: i32,
    /// When true, renders the full worldspace terrain horizon across all surrounding cells.
    pub full_terrain: bool,
    /// The "new cell" form's fields.
    ///
    /// The form's state has to live somewhere that survives the frame: an
    /// egui text field built from a local string is discarded unread, so
    /// whatever the user types would vanish as soon as they clicked elsewhere.
    pub new_cell: NewCellForm,
    pub new_cell_full: String,
    /// The worldspace the new cell goes in. `None` makes it an interior cell.
    pub new_cell_world: Option<u32>,
    pub new_cell_grid_x: i32,
    pub new_cell_grid_y: i32,
    pub new_cell_with_land: bool,
    /// Whether the "new cell" form is showing.
    pub new_cell_open: bool,
    /// The "new worldspace" form's fields.
    pub new_world: NewWorldForm,
    pub new_world_full: String,
    pub new_world_land: f32,
    pub new_world_water: f32,
    /// Whether the "new worldspace" form is showing.
    pub new_world_open: bool,
    /// The duplicate prompt: the record being copied and the name typed for it.
    pub duplicate: Option<DuplicatePrompt>,
}

/// The prompt the object window's "Duplicate" opens: what is being copied,
/// and the Editor ID the copy will carry.
///
/// The prefill is the original's Editor ID with `COPY` on it, so the common
/// case — one copy, right now — is a single keystroke. It is state because the
/// field has to survive the frames while the user types into it.
#[derive(Debug, Clone)]
pub struct DuplicatePrompt {
    /// The record being copied.
    pub fid: u32,
    /// Its signature, which the copy keeps.
    pub sig: [u8; 4],
    /// The Editor ID the copy will be written with, pre-filled and editable.
    pub edid: String,
}

/// The Editor ID of a cell the user is creating.
#[derive(Debug, Clone, Default)]
pub struct NewCellForm {
    pub edid: String,
}

/// The Editor ID of a worldspace the user is creating.
#[derive(Debug, Clone, Default)]
pub struct NewWorldForm {
    pub edid: String,
}

impl Default for Filters {
    fn default() -> Self {
        Self {
            cell: String::new(),
            place: String::new(),
            reference: String::new(),
            reference_type: None,
            cell_scope: CellScope::default(),
            exterior_radius: 2,
            full_terrain: true,
            new_cell: NewCellForm::default(),
            new_cell_full: String::new(),
            new_cell_world: None,
            new_cell_grid_x: 0,
            new_cell_grid_y: 0,
            new_cell_with_land: true,
            new_cell_open: false,
            new_world: NewWorldForm::default(),
            new_world_full: String::new(),
            new_world_land: -2500.0,
            new_world_water: -2300.0,
            new_world_open: false,
            duplicate: None,
        }
    }
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

/// A vertical scroll area whose bar is pinned to the panel's right edge.
///
/// egui's default `auto_shrink` is `[true, true]`: the area shrinks to the
/// *content's* size and the bar is painted at the area's own right edge. A grid
/// of labelled fields is narrower than the window it sits in, so the bar landed
/// wherever the widest row happened to end — mid-panel, and it moved as the
/// user changed record or as a longer value widened a column.
///
/// Taking the width from the panel instead of from the content (`auto_shrink`'s
/// x off, y left on) puts the bar at the window's edge and keeps it still. The
/// vertical shrink stays on because a scroll area that is taller than its
/// content should not reserve the empty space below it.
///
/// The bar's width and opacity are not set here: egui keeps those in the style,
/// not on the `ScrollArea`, so [`crate::windows::widen_scroll_bars`] sets them
/// for the whole app.
pub fn vertical_scroll(id_salt: &str) -> egui::ScrollArea {
    egui::ScrollArea::vertical()
        .id_salt(id_salt)
        .auto_shrink([false, true])
}

/// The main window: the editor's front door.
///
/// The viewport and every panel own a window of their own, so this one holds only
/// what has no better home: where the data came from, and the actions that are
/// about the whole plugin rather than the open cell.
pub fn main_window(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<bevy_egui::PrimaryEguiContext>>,
    mut session: ResMut<crate::session::Session>,
    game: Res<crate::Game>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "main"), |ui| {
        ui.heading(game.0.editor_name);
        ui.label(format!("game    {}", game.0.name));
        ui.label(format!("plugin  {}", session.doc.name()));
        // Where a save writes, spelled out: a save that lands somewhere the user
        // did not expect is only findable if the panel said where it went.
        ui.label(format!("save    {}", session.save_target().display()));
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
        // The create buttons go first, above the cell list: the list is a
        // scroll area that fills the window, so anything after it is out of
        // sight.
        new_cell_ui(ui, &mut session, &mut filters, &worlds);
        ui.separator();
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
        if filters.cell_scope != CellScope::Interiors {
            ui.horizontal(|ui| {
                ui.label("Objects:");
                let label = match filters.exterior_radius {
                    0 => "1x1 (Cell only)",
                    1 => "3x3",
                    2 => "5x5 (Zone)",
                    3 => "7x7",
                    4 => "9x9",
                    -1 => "All (Worldspace)",
                    _ => "Custom",
                };
                egui::ComboBox::from_id_salt("objects_radius")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut filters.exterior_radius, 0, "1x1 (Cell only)");
                        ui.selectable_value(&mut filters.exterior_radius, 1, "3x3");
                        ui.selectable_value(&mut filters.exterior_radius, 2, "5x5 (Zone)");
                        ui.selectable_value(&mut filters.exterior_radius, 3, "7x7");
                        ui.selectable_value(&mut filters.exterior_radius, 4, "9x9");
                        ui.selectable_value(&mut filters.exterior_radius, -1, "All (Worldspace)");
                    });
                ui.separator();
                ui.checkbox(&mut filters.full_terrain, "Full landscape");
            });
        }
        let filter = filters.cell.clone();
        let scope = filters.cell_scope;
        let cells = session.cells(scope);
        // Filtered here rather than in the session: the session's list is what
        // the dropdown counts, and the count has to say what the scope holds
        // rather than what the filter leaves.
        let rows: Vec<&crate::session::CellRow> =
            cells.iter().filter(|c| row_matches(c, &filter)).collect();
        let shown = cells.len();
        let matched = rows.len();
        // What the row context menus chose. Both run after the table, because
        // the table borrows the session and the actions take it.
        let mut edit_cell: Option<u32> = None;
        let mut open_cell: Option<u32> = None;
        // A header row, then one row per cell, in a table whose columns have
        // a width policy each — the Editor ID clips, the Name takes what is
        // left — so one long name cannot stretch the whole list. Clicking a
        // row opens that cell, as the list did.
        egui_extras::TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .resizable(true)
            .id_salt("cells_table")
            .column(
                egui_extras::Column::initial(260.0)
                    .at_least(140.0)
                    .clip(true),
            )
            .column(egui_extras::Column::initial(90.0).at_least(80.0))
            .column(egui_extras::Column::remainder().resizable(false).clip(true))
            .column(egui_extras::Column::initial(110.0).at_least(90.0))
            .header(18.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Editor ID");
                });
                header.col(|ui| {
                    ui.strong("Form ID");
                });
                header.col(|ui| {
                    ui.strong("Name");
                });
                header.col(|ui| {
                    ui.strong("Location");
                });
            })
            .body(|body| {
                body.rows(18.0, rows.len(), |mut row_ui| {
                    let c = &rows[row_ui.index()];
                    let open = session.cell == Some(c.form_id);
                    // The whole row is the widget: it highlights whole when
                    // it is the open cell, and a click anywhere on it opens
                    // the cell — the way the old list's one clickable name
                    // did, over the full row instead.
                    row_ui.set_selected(open);
                    row_ui.col(|ui| {
                        ui.label(c.editor_id()).on_hover_text(format!(
                            "open {} ({} refs)",
                            c.editor_id(),
                            c.refs
                        ));
                    });
                    let resp = row_ui.response();
                    if resp.clicked() {
                        session.open_cell(c.form_id);
                    }
                    // Right-clicking a row opens its context menu. Edit is
                    // what opens the Cell Editor window on this cell; the
                    // menu is also where the row is opened in the viewport
                    // without a left click, which is what GECK's own Cell
                    // View offers.
                    resp.context_menu(|ui| {
                        if ui.button("Edit").clicked() {
                            edit_cell = Some(c.form_id);
                            ui.close();
                        }
                        if ui.button("Open in viewport").clicked() {
                            open_cell = Some(c.form_id);
                            ui.close();
                        }
                    });
                    // The fixed-width columns are monospace, so the Form IDs
                    // and the grid coordinates line up down the column.
                    row_ui.col(|ui| {
                        ui.label(egui::RichText::new(format!("{:08X}", c.form_id)).monospace());
                    });
                    row_ui.col(|ui| {
                        ui.label(c.name.as_deref().unwrap_or(""));
                    });
                    row_ui.col(|ui| {
                        ui.label(egui::RichText::new(c.location()).monospace());
                    });
                });
            });
        // The menu's actions, after the table that chose them.
        //
        // Open before edit: a row that is opened in the viewport is also the
        // one whose properties are worth editing, and the two are separate
        // choices — the menu offers them separately.
        if let Some(fid) = open_cell {
            session.open_cell(fid);
        }
        if let Some(fid) = edit_cell {
            session.open_cell_editor(fid);
        }
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
        ui.separator();
        // What the session has staged, so the user can see what a save will
        // write. A staged cell is not in the cell list yet — the list reads
        // the load order, and the cell only exists in the file once it is
        // saved — so it is listed here instead.
        let staged_cells: Vec<(String, String)> = session
            .staged_cells()
            .iter()
            .map(|c| {
                let where_ = match (c.spec.world, c.spec.grid) {
                    (Some(_), Some((x, y))) => format!("grid {x}, {y}"),
                    (Some(_), None) => "exterior".to_string(),
                    _ => "interior".to_string(),
                };
                (c.spec.edid.clone(), where_)
            })
            .collect();
        let staged_worlds: Vec<String> = session
            .staged_worldspaces()
            .iter()
            .map(|w| w.spec.edid.clone())
            .collect();
        if !staged_worlds.is_empty() {
            ui.label(egui::RichText::new("New worldspaces (not yet saved)").strong());
            for w in &staged_worlds {
                ui.label(format!("  {w}"));
            }
        }
        if !staged_cells.is_empty() {
            ui.label(egui::RichText::new("New cells (not yet saved)").strong());
            for (edid, where_) in &staged_cells {
                ui.label(format!("  {edid}  ({where_})"));
            }
        }
    });
}

/// The "new cell" and "new worldspace" controls, and the cell search box.
///
/// GECK's Cell View has a "Create" button that makes a new cell in the
/// current worldspace. This is the same: two buttons that open a form, and
/// the form that stages the cell or worldspace for the next save.
///
/// The buttons sit at the **top** of the Cells window, next to the scope
/// dropdown, because the cell list below is a scroll area that fills the
/// window — anything after it is pushed out of sight.
///
/// The cell filter's text box lives in this same row, to the right of the
/// "+ New worldspace" button, for the same reason: it narrows the list below,
/// so it belongs with the list's controls and has to stay on screen. The
/// string it edits is [`Filters::cell`], which the list below reads next
/// frame, so nothing is copied into a local that would be dropped.
fn new_cell_ui(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    filters: &mut Filters,
    worlds: &[crate::session::WorldspaceRow],
) {
    let mut new = filters.new_cell.clone();
    let mut world = filters.new_cell_world;
    let mut grid_x = filters.new_cell_grid_x;
    let mut grid_y = filters.new_cell_grid_y;
    let mut with_land = filters.new_cell_with_land;
    let mut full = filters.new_cell_full.clone();
    let mut cell_open = filters.new_cell_open;
    let mut world_open = filters.new_world_open;

    // The two buttons and the cell search box, in a row. The search box is the
    // "filter" line that used to sit below the table, moved up here: it filters
    // the list, so it belongs next to the list's other controls, and putting it
    // above the scroll area keeps it on screen at every window height.
    ui.horizontal(|ui| {
        if ui
            .selectable_label(cell_open, egui::RichText::new("+ New cell").strong())
            .clicked()
        {
            cell_open = !cell_open;
            if cell_open {
                world_open = false;
            }
        }
        if ui
            .selectable_label(world_open, egui::RichText::new("+ New worldspace").strong())
            .clicked()
        {
            world_open = !world_open;
            if world_open {
                cell_open = false;
            }
        }
        // The box takes what the row has left, and never less than a readable
        // width: a squeezed search box is worse than no search box. Its hint
        // names the columns it searches, because that is not guessable from
        // the box itself.
        ui.add(
            egui::TextEdit::singleline(&mut filters.cell)
                .hint_text("editor ID, name, FormID, grid")
                .desired_width(ui.available_width().max(180.0)),
        );
    });

    if cell_open {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Editor ID");
            ui.text_edit_singleline(&mut new.edid);
        });
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut full);
        });
        ui.horizontal(|ui| {
            ui.label("Worldspace");
            // The dropdown offers the interiors and every worldspace, and the
            // scope the cell browser is showing is the natural default.
            egui::ComboBox::from_id_salt("new_cell_world")
                .selected_text(match world {
                    None => "Interior".to_string(),
                    Some(w) => worlds
                        .iter()
                        .find(|x| x.form_id == w)
                        .map(|x| x.label.clone())
                        .unwrap_or_else(|| format!("{w:08X}")),
                })
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut world, None, "Interior");
                    for w in worlds {
                        ui.selectable_value(&mut world, Some(w.form_id), &w.label);
                    }
                });
        });
        // The grid is only meaningful for an exterior cell: the game reads a
        // cell's grid to place it, and an interior cell has none.
        if world.is_some() {
            ui.horizontal(|ui| {
                ui.label("Grid");
                ui.add(egui::DragValue::new(&mut grid_x).range(-1000..=1000));
                ui.label(",");
                ui.add(egui::DragValue::new(&mut grid_y).range(-1000..=1000));
            });
            ui.checkbox(&mut with_land, "Give it terrain (LAND)");
        }
        let exterior = world.is_some();
        if ui.button("Create cell").clicked() {
            let name = if full.trim().is_empty() {
                None
            } else {
                Some(full.clone())
            };
            let grid = world.map(|_| (grid_x, grid_y));
            match session.create_cell(&new.edid, name, world, grid, with_land && exterior) {
                Ok(fid) => {
                    new.edid.clear();
                    full.clear();
                    cell_open = false;
                    session.status = format!("created cell {:08X}; save to write it", fid);
                }
                Err(e) => session.status = format!("could not create the cell: {e}"),
            }
        }
    }

    if world_open {
        ui.separator();
        let mut new_world = filters.new_world.clone();
        let mut world_full = filters.new_world_full.clone();
        let mut land = filters.new_world_land;
        let mut water = filters.new_world_water;
        ui.horizontal(|ui| {
            ui.label("Editor ID");
            ui.text_edit_singleline(&mut new_world.edid);
        });
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut world_full);
        });
        ui.horizontal(|ui| {
            ui.label("Land height");
            ui.add(egui::DragValue::new(&mut land).speed(10.0));
            ui.label("Water height");
            ui.add(egui::DragValue::new(&mut water).speed(10.0));
        });
        if ui.button("Create worldspace").clicked() {
            let name = if world_full.trim().is_empty() {
                None
            } else {
                Some(world_full.clone())
            };
            match session.create_worldspace(&new_world.edid, name, land, water) {
                Ok(fid) => {
                    new_world.edid.clear();
                    world_full.clear();
                    world_open = false;
                    session.status = format!("created worldspace {:08X}; save to write it", fid);
                }
                Err(e) => session.status = format!("could not create the worldspace: {e}"),
            }
        }
        // Keep what the user typed: the form's fields are state, not local
        // variables that vanish when the frame ends.
        filters.new_world = new_world;
        filters.new_world_full = world_full;
        filters.new_world_land = land;
        filters.new_world_water = water;
    }

    filters.new_cell = new;
    filters.new_cell_full = full;
    filters.new_cell_world = world;
    filters.new_cell_grid_x = grid_x;
    filters.new_cell_grid_y = grid_y;
    filters.new_cell_with_land = with_land;
    filters.new_cell_open = cell_open;
    filters.new_world_open = world_open;
}

/// The references in the open cell: a table, a search box and a type filter.
///
/// GECK's render window lists a cell's references as a table — Editor ID, Form
/// ID, type, owner, lock — with a filter above it. This is the same five
/// columns: the search box narrows on any of the first four, and the dropdown
/// narrows on the type, which is what the reference *places* — "Static",
/// "Activator" — rather than the reference's own REFR.
pub fn references(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<1>>>,
    mut session: ResMut<crate::session::Session>,
    mut filters: ResMut<Filters>,
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
        // The rows and the resolved columns are read out before the loop, which
        // needs `session` mutably to change the selection.
        let refs = session.cell_refs().to_vec();
        let details = session.ref_details().clone();
        let queued = session.deleted().to_vec();
        let total = refs.len();

        // The type dropdown's options are the cell's own base types, in file
        // order: a cell holds a handful of kinds, and listing every record type
        // in the game would bury the four that are there.
        let mut types: Vec<String> = Vec::new();
        for r in &refs {
            let Some(base) = r.base.as_deref().and_then(parse_formid) else {
                continue;
            };
            if let Some(t) = details.base_type(base) {
                if !types.iter().any(|s| s == t) {
                    types.push(t.to_string());
                }
            }
        }
        let type_filter = filters.reference_type.clone();
        let filter = filters.reference.to_lowercase();

        // A row matches when the text is in any column and the dropdown either
        // names its type or names none.
        //
        // A reference with no base object has no type to filter on, so a type
        // filter hides it — but with no filter chosen it still shows: dropping
        // rows the filter did not ask about would make the table's count lie.
        let rows: Vec<&crate::session::RefRow> = refs
            .iter()
            .filter(|r| {
                if let Some(want) = &type_filter {
                    let base = r.base.as_deref().and_then(parse_formid);
                    if base.and_then(|b| details.base_type(b)) != Some(want.as_str()) {
                        return false;
                    }
                }
                filter.is_empty() || ref_row_matches(r, &details, &filter)
            })
            .collect();
        let matched = rows.len();

        ui.label(format!("{name}: {total} refs"));
        ui.horizontal(|ui| {
            ui.label("filter");
            ui.add(egui::TextEdit::singleline(&mut filters.reference).desired_width(90.0));
            let selected = type_filter.as_deref().unwrap_or("all types").to_string();
            egui::ComboBox::from_id_salt("reference_type")
                .selected_text(selected)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut filters.reference_type, None, "all types");
                    for t in &types {
                        ui.selectable_value(&mut filters.reference_type, Some(t.clone()), t);
                    }
                });
        });
        ui.separator();
        // A table with a width policy per column — the Editor ID is the
        // remainder and absorbs the slack, so a long base name cannot push
        // the lock column out of the pane; the other four start at a
        // preset width and can be dragged. The row is the widget: it
        // highlights whole when selected, and a click anywhere on it
        // selects.
        egui_extras::TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .resizable(true)
            .id_salt("references_table")
            .column(
                egui_extras::Column::remainder()
                    .resizable(false)
                    .at_least(160.0)
                    .clip(true),
            )
            .column(egui_extras::Column::initial(90.0).at_least(80.0))
            .column(egui_extras::Column::initial(110.0).at_least(80.0))
            .column(
                egui_extras::Column::initial(130.0)
                    .at_least(90.0)
                    .clip(true),
            )
            .column(egui_extras::Column::initial(70.0).at_least(50.0))
            .header(18.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Editor ID");
                });
                header.col(|ui| {
                    ui.strong("Form ID");
                });
                header.col(|ui| {
                    ui.strong("Type");
                });
                header.col(|ui| {
                    ui.strong("Owner");
                });
                header.col(|ui| {
                    ui.strong("Lock");
                });
            })
            .body(|body| {
                body.rows(18.0, rows.len(), |mut row_ui| {
                    let r = &rows[row_ui.index()];
                    let open = session.selected == Some(r.form_id);
                    let base = r.base.as_deref().and_then(parse_formid);
                    // A placed object is named after what it places. Only
                    // 5,011 of FalloutNV's 307,710 REFRs carry an EDID of
                    // their own [observed: fnvtool esm survey], so a row
                    // with none shows its base object's Editor ID — the
                    // "WastelandRoadStraightLongB" GECK's render window
                    // shows — and a reference whose base names nothing
                    // falls back to its own signature and FormID.
                    let edid = r
                        .edid
                        .clone()
                        .or_else(|| base.and_then(|b| details.base_edid(b)).map(str::to_string))
                        .unwrap_or_else(|| format!("{} {:08X}", r.sig, r.form_id));
                    // A persistent reference is marked with a star, the way a
                    // mod list marks a master.
                    let mut text = if r.persistent {
                        format!("{edid} *")
                    } else {
                        edid
                    };
                    if queued.contains(&r.form_id) {
                        text = format!("{text}  [deleting]");
                    }
                    row_ui.set_selected(open);
                    row_ui.col(|ui| {
                        ui.label(text);
                    });
                    let resp = row_ui.response();
                    if resp.clicked() {
                        session.select(r.form_id);
                        // A double-click on a row frames it in the viewport,
                        // the same as double-clicking the model itself. The
                        // click state is this panel's own, so a double-click
                        // in the viewport and one here do not interfere.
                        if clicks.register(r.form_id) {
                            frame.write(crate::FrameRequested { form_id: r.form_id });
                        }
                    }
                    // The fixed-width columns are monospace, so the Form IDs
                    // and the lock levels line up down the column.
                    row_ui.col(|ui| {
                        ui.label(egui::RichText::new(format!("{:08X}", r.form_id)).monospace());
                    });
                    // The type is what the base object is, as GECK names
                    // it — "Static", "Activator". A reference whose base is
                    // missing has nothing to say, so the column stays empty
                    // rather than showing the reference's own signature for
                    // some rows and not others.
                    let type_cell = base
                        .and_then(|b| details.base_type(b))
                        .unwrap_or("")
                        .to_string();
                    row_ui.col(|ui| {
                        ui.label(type_cell);
                    });
                    // The owner's name, or the FormID when the record has no
                    // Editor ID to show.
                    let owner_cell = r
                        .owner
                        .and_then(|o| details.owner(o))
                        .unwrap_or("")
                        .to_string();
                    row_ui.col(|ui| {
                        ui.label(owner_cell);
                    });
                    let lock_cell = r.lock.map(|l| l.label()).unwrap_or_default();
                    row_ui.col(|ui| {
                        ui.label(egui::RichText::new(lock_cell).monospace());
                    });
                });
            });
        ui.separator();
        ui.label(format!(
            "{total} refs in {name}{}",
            if matched == total {
                String::new()
            } else {
                format!(" ({matched} match)")
            }
        ));
        if ui.button("Delete selected").clicked() {
            session.delete_selected();
        }
    });
}

/// Parse the decoder's FormID text form ("0x0001EE8F").
///
/// The prefix is optional, so a bare `0001EE8F` is read as the same FormID:
/// a FormID is a number, and which of its two spellings the user typed should
/// not change what it is. Anything that is not hex is not a FormID, which is
/// what keeps a word from matching as the number 0.
fn parse_formid(s: &str) -> Option<u32> {
    let hex = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

/// Whether one cell row's columns contain the filter text.
///
/// Every column the table shows is searched, not just the first: the Editor ID
/// is what a modded cell is named by, the display name is what GECK shows, the
/// Form ID is the only way to name a cell that has neither — which is most of
/// FalloutNV's exterior cells — and the grid square is how the user thinks of
/// an exterior cell's place. The FormID is matched in the spelling the table
/// uses, `0031E2A`, and the `0x` form the decoder writes is accepted too.
///
/// A partial hex string narrows on the digits typed so far rather than on the
/// whole FormID: `0031` is the first four digits of a column of eight-digit
/// numbers, and the user is typing left to right. The name columns are matched
/// case-insensitively, the hex ones exactly as shown. An explicit `0x` is a
/// FormID and nothing else — no name or location column starts that way.
pub fn row_matches(c: &crate::session::CellRow, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let form_id = format!("{:08X}", c.form_id);
    // An explicit 0x is a FormID and nothing else: no name or location column
    // starts that way, so it cannot be a substring of one.
    if let Some(hex) = filter.strip_prefix("0x") {
        return form_id
            .get(..hex.len().min(form_id.len()))
            .is_some_and(|head| head.eq_ignore_ascii_case(hex));
    }
    let lower = filter.to_lowercase();
    c.editor_id().to_lowercase().contains(&lower)
        || c.name
            .as_deref()
            .is_some_and(|n| n.to_lowercase().contains(&lower))
        || form_id.contains(filter)
        // The grid square as the column shows it, `-4, 7` and `-4,7` alike: the
        // space after the comma is one character the user may or may not type.
        // It is matched in full, not as a substring, because `4, 7` is a
        // substring of `-4, 7` and a search that matched every cell in the
        // left-hand column would find nothing the user was looking for.
        || c.location().replace(' ', "") == lower.replace(' ', "")
}

/// Whether one reference row's columns contain the filter text, lower-cased.
///
/// The Editor ID, the FormID, the base object's Editor ID and the owner's name
/// are all searched: a cell's references mostly have no Editor ID of their own,
/// so matching only that column would make most rows unfindable.
fn ref_row_matches(
    r: &crate::session::RefRow,
    details: &crate::session::RefDetails,
    filter: &str,
) -> bool {
    let edid = r.edid.as_deref().unwrap_or("");
    let owner = r.owner.and_then(|o| details.owner(o)).unwrap_or("");
    let form_id = format!("{:08X}", r.form_id);
    let base = r
        .base
        .as_deref()
        .and_then(parse_formid)
        .and_then(|b| details.base_edid(b))
        .unwrap_or("");
    edid.to_lowercase().contains(filter)
        || form_id.contains(filter)
        || base.to_lowercase().contains(filter)
        || owner.to_lowercase().contains(filter)
}

/// The property grid for the selected reference.
///
/// Every subrecord of the selected record is listed, and every field inside it
/// that has a scalar editor — a number, a string, a flag, an enum — is editable
/// in place. That is the whole point of the panel: what the schema describes,
/// the game accepts, and the encoder can write back is editable, so nothing is
/// hidden behind a raw JSON blob.
///
/// A subrecord whose layout cannot round-trip (a repeated struct field, an
/// unknown signature) is shown read-only with the reason, rather than being
/// dropped: knowing a field exists and cannot be written is itself the answer
/// to "why can I not edit this".
pub fn properties(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<2>>>,
    mut session: ResMut<crate::session::Session>,
    mut edit: Local<FieldEdit>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "properties"), |ui| {
        let Some(fid) = session.selected else {
            ui.label("nothing selected");
            return;
        };
        // Everything the grid shows is read out before the mutable calls below,
        // so the record's borrow ends here. It is the record as the mod will
        // carry it — queued edits applied — so the header and the grid cannot
        // disagree about what the record says.
        let Some(rec) = session.resolved_record(fid) else {
            ui.label(format!("{fid:08X} not found"));
            return;
        };
        let sig_label = rec.sig_str();
        let pos = crate::session::position_of(&rec);
        ui.label(format!("{:08X} {}", fid, sig_label));
        if session.pending_edits() > 0 {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!("{} unsaved", session.pending_edits()),
            );
        }
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
        crate::panels::vertical_scroll("properties_grid").show(ui, |ui| {
            schema_grid(ui, &mut session, fid, &mut edit);
        });
    });
}

/// The schema grid, with the subrecords grouped under the schema's headings.
///
/// A described page: `sections` is the game's own table for this record type,
/// so what is in it is data. Each section draws the subrecords it names, in the
/// order it names them, and a subrecord the record does not carry is skipped —
/// which is how one section list serves games whose records differ.
///
/// Everything the sections did **not** name follows under one heading. That is
/// not a formality: a page that dropped a subrecord would leave part of the
/// record uneditable with nothing saying so, so the unlisted remainder is shown
/// rather than trusted not to exist. A record type whose sections cover
/// everything shows nothing there.
pub fn sectioned_grid(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    form_id: u32,
    sections: &[nvre_esm::schema::SectionDef],
    edit: &mut FieldEdit,
) {
    sectioned_grid_on_tab(ui, session, form_id, sections, None, edit, None)
}

/// The tab strip above a tabbed page's body.
///
/// A strip of selectable labels rather than egui's dockable tab panels: these are
/// not dockable, and a button strip is the lighter thing — the same choice the
/// cell editor's strip makes, so the editor has one kind of tab strip rather than
/// two.
///
/// `chosen` is the chosen **heading**, and the strip writes it back. A heading the
/// strip does not carry cannot be chosen through the strip, so `chosen` stays a
/// valid answer: the fallback in [`crate::object_editor::EditorTab::heading`] is
/// for a schema that dropped a tab between frames.
///
/// The strip wraps rather than scrolls horizontally: a creature has thirteen
/// tabs, and a strip that ran off the window's edge would hide the last ones with
/// no way to reach them.
pub fn tab_strip(ui: &mut egui::Ui, tabs: &[String], chosen: &mut Option<String>) {
    let current = match chosen {
        Some(name) if tabs.iter().any(|t| t == name) => name.clone(),
        _ => tabs.first().cloned().unwrap_or_default(),
    };
    ui.horizontal_wrapped(|ui| {
        for tab in tabs {
            if ui
                .selectable_label(current == *tab, egui::RichText::new(tab).strong())
                .clicked()
            {
                *chosen = Some(tab.clone());
            }
        }
    });
    ui.separator();
}

/// The sections a tab draws, from a page's full section list.
///
/// A page with no strip (`tab == None`) draws them all. A tabbed page draws the
/// sections filed on the chosen tab plus the ones filed on none — the record's
/// shared part, which belongs on every tab.
///
/// Public because it is the rule the page's rendering follows, and the tests that
/// keep a split honest call it rather than rendering a window.
pub fn sections_on_tab<'a>(
    sections: &'a [nvre_esm::schema::SectionDef],
    tab: Option<&str>,
) -> Vec<&'a nvre_esm::schema::SectionDef> {
    sections
        .iter()
        .filter(|s| match (&s.tab, tab) {
            (None, _) => true,
            (Some(_), None) => true,
            (Some(name), Some(chosen)) => name == chosen,
        })
        .collect()
}

/// The same grid, narrowed to one tab of a tabbed page.
///
/// `tab` is the chosen tab's heading, or `None` for a page with no strip. A
/// section filed on another tab is skipped, so the modder sees the part of the
/// record the tab is about rather than the whole record under a tab that claims
/// to be about one thing. A section with no tab at all is drawn on every tab: it
/// is the record's shared part.
///
/// The chosen tab is passed in rather than read from the session because the
/// page's own tab belongs to the window, not to the record — two creature editors
/// open on two records have two tabs.
pub fn sectioned_grid_on_tab(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    form_id: u32,
    sections: &[nvre_esm::schema::SectionDef],
    tab: Option<&str>,
    edit: &mut FieldEdit,
    bounds_out: Option<&std::sync::Mutex<Option<egui::Rect>>>,
) {
    let Some(rec) = session.resolved_record(form_id) else {
        ui.colored_label(egui::Color32::RED, format!("{form_id:08X} not found"));
        return;
    };
    let rec_sig = rec.header.sig;
    let views =
        sub_views(&session.schema, &rec, session.record_is_localized(form_id)).unwrap_or_default();
    if views.is_empty() {
        ui.label("the schema describes no subrecords for this record");
        return;
    }
    if let Some(err) = &edit.error {
        ui.colored_label(egui::Color32::RED, err);
    }
    let edited: Vec<[u8; 4]> = session
        .doc
        .edits_for(form_id)
        .unwrap_or_default()
        .iter()
        .filter_map(|e| match e {
            nvre_edit::document::Edit::SetSub { sig, .. } => Some(*sig),
            _ => None,
        })
        .collect();
    // The sections this tab draws. A page with no strip draws them all; a tabbed
    // page draws the sections filed on the chosen tab plus the ones filed on none.
    let shown = sections_on_tab(sections, tab);
    // Which views a section claimed, so the remainder is what nothing named.
    // The remainder is judged against **every** section, not just this tab's: a
    // subrecord whose section sits on another tab is described, and showing it
    // again here under "Everything else" would put the same field on two tabs.
    let mut claimed: Vec<usize> = Vec::new();
    for section in sections {
        for tag in &section.subs {
            for (i, v) in views.iter().enumerate() {
                if &v.sig == tag && !claimed.contains(&i) {
                    claimed.push(i);
                }
            }
        }
    }
    enum SecChunk<'a> {
        Single((usize, &'a nvre_esm::schema::SectionDef)),
        SideBySide(Vec<(usize, &'a nvre_esm::schema::SectionDef)>),
    }
    let mut chunks: Vec<SecChunk> = Vec::new();
    for (si, section) in shown.iter().enumerate() {
        let has_subs = section.subs.iter().any(|tag| views.iter().any(|v| &v.sig == tag));
        // A widget that owns its own list has to draw even when the list is
        // empty: `repeat` shows its "+ Add Faction" button on a record with no
        // factions, and `inventory` shows "+ Add Item" on a record with no
        // items. Skipping them would make an empty list unaddable.
        let is_repeat = section.widget.as_deref() == Some("repeat");
        let is_inventory = section.widget.as_deref() == Some("inventory");
        let is_animations = section.widget.as_deref() == Some("animations");
        let is_model_list = section.widget.as_deref() == Some("model_list");
        let is_object_bounds = section.widget.as_deref() == Some("object_bounds");
        // A section whose every subrecord is absent still has something to
        // draw: the absent-subrecord picker, which is how a field the record
        // does not carry yet gets set. `EITM` is a FormID and `EAMT` an enum,
        // so both have a widget to offer even with no bytes on the record —
        // but only when the schema describes the subrecord for this record
        // type at all, which is what separates "not on this record" from "not
        // a subrecord this record has".
        let all_described = !section.subs.is_empty()
            && section
                .subs
                .iter()
                .all(|tag| session.schema.lookup(&rec_sig, &sig4(tag)).is_some());
        if !has_subs
            && !is_repeat
            && !is_inventory
            && !is_animations
            && !is_model_list
            && !is_object_bounds
            && !all_described
        {
            continue;
        }
        if section.side_by_side {
            match chunks.last_mut() {
                Some(SecChunk::SideBySide(list)) => list.push((si, section)),
                _ => chunks.push(SecChunk::SideBySide(vec![(si, section)])),
            }
        } else {
            chunks.push(SecChunk::Single((si, section)));
        }
    }

    for (ci, chunk) in chunks.iter().enumerate() {
        if ci > 0 {
            ui.add_space(6.0);
        }
        match chunk {
            SecChunk::Single((si, section)) => {
                render_single_section(
                    ui,
                    session,
                    form_id,
                    rec_sig,
                    &views,
                    &edited,
                    edit,
                    *si,
                    section,
                    bounds_out,
                );
            }
            SecChunk::SideBySide(list) => {
                ui.horizontal_top(|ui| {
                    for (idx, (si, section)) in list.iter().enumerate() {
                        if idx > 0 {
                            ui.add_space(8.0);
                        }
                        ui.vertical(|ui| {
                            render_single_section(
                                ui,
                                session,
                                form_id,
                                rec_sig,
                                &views,
                                &edited,
                                edit,
                                *si,
                                section,
                                bounds_out,
                            );
                        });
                    }
                });
            }
        }
    }
    // "Everything else" is the subrecords no section names. The object editor's
    // header owns a few of them — the object bounds, the model, the script — and
    // draws its own editor for them above the page, so a second copy here would
    // be two widgets writing one subrecord. They are excluded rather than
    // hidden: a record whose page names nothing still gets everything else it
    // carries, and the header's own editor is where these are edited.
    // A subrecord is only left out of "Everything else" when some section
    // already draws it — otherwise it would be unreachable. The creature's
    // `MODL` and `OBND` are on its Model List tab, so a second copy under
    // "Everything else" would be two widgets writing one subrecord. A `STAT`
    // has no such tab and names neither, so hiding them would leave them
    // uneditable with nothing saying so, which is the one failure this editor
    // exists not to have.
    let rest: Vec<usize> = (0..views.len())
        .filter(|i| !claimed.contains(i))
        .collect();
    if !rest.is_empty() {
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.strong("Everything else");
            ui.weak("subrecords the schema's sections do not name");
            ui.add_space(4.0);
            for i in rest {
                subrecord_rows(ui, session, form_id, &rec_sig, &views[i], &edited, edit);
            }
        });
    }
}

/// One subrecord of a record: its heading, then a row per field.
///
/// Shared by the flat grid and the sectioned one so the two cannot drift: a
/// field that is editable in one is editable in the other, and a subrecord that
/// cannot be written says why in both.
fn subrecord_rows(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    form_id: u32,
    rec_sig: &[u8; 4],
    view: &nvre_edit::encode::SubView,
    edited: &[[u8; 4]],
    edit: &mut FieldEdit,
) {
    subrecord_rows_on_fields(
        ui,
        session,
        form_id,
        rec_sig,
        view,
        edited,
        None,
        &std::collections::BTreeMap::new(),
        false,
        edit,
    );
}

/// The same, narrowed to the fields a section names.
///
/// `fields` is a list of dotted paths for **this** subrecord, or `None` for "every
/// field it has". Narrowing is what lets one subrecord's fields sit on two tabs:
/// the game's own editor splits `ACBS` between Traits and Stats, and a page that
/// cannot split it either shows the flags twice or shows them on neither.
///
/// The order is the section's, not the layout's, because a split is a claim about
/// which fields belong together. A path the decoded subrecord does not carry is
/// skipped — the same rule a section naming an absent subrecord follows — so a
/// game whose `ACBS` lacks a field still gets the rest.
#[allow(clippy::too_many_arguments)]
fn subrecord_rows_on_fields(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    form_id: u32,
    rec_sig: &[u8; 4],
    view: &nvre_edit::encode::SubView,
    edited: &[[u8; 4]],
    fields: Option<&[String]>,
    flags_bits: &std::collections::BTreeMap<String, Vec<u32>>,
    untitled: bool,
    edit: &mut FieldEdit,
) {
    let sig = sig4(&view.sig);
    let rows = nvre_edit::encode::rows(
        &session.schema,
        rec_sig,
        &sig,
        view.block.as_deref(),
        &view.value,
    );
    // The subrecord's heading: its name, its signature, and where it sits. A
    // subrecord that is **one** field — `EDID`, `FULL`, `SCRI`, a bare FormID —
    // does not need it: the row below already carries the field's own name, and
    // the heading would repeat it with the signature bolted on. Three labels
    // for one text box is what the creature's Identity section looked like.
    //
    // A subrecord with several fields keeps its heading, because that is what
    // groups them: `ACBS` under "Configuration ACBS" is one heading for the
    // flags, the level and the rest.
    //
    // The count is of the rows the section **shows**, not the ones the subrecord
    // has: a section that narrows `ACBS` to its flags alone shows one row, and
    // a heading reading "Configuration ACBS" above a lone checkbox row is the
    // same redundancy.
    let shown_count = match fields {
        None => rows.len(),
        Some(paths) => paths
            .iter()
            .filter(|p| rows.iter().any(|r| &r.path == *p))
            .count(),
    };
    if !untitled && fields.is_none() && shown_count > 1 {
        ui.horizontal(|ui| {
            ui.strong(&view.name);
            ui.monospace(&view.sig);
            if let Some(block) = &view.block {
                ui.weak(format!("in {block}"));
            }
            if edited.contains(&sig) {
                ui.colored_label(egui::Color32::LIGHT_BLUE, "edited");
            } else if !view.writable {
                ui.colored_label(egui::Color32::YELLOW, "read-only");
            }
        });
    }
    if let Some(p) = &view.problem {
        ui.colored_label(egui::Color32::RED, p);
    }
    // The rows this section shows, in the section's order when it narrows and in
    // the layout's when it does not.
    let shown: Vec<nvre_edit::encode::Row> = match fields {
        None => rows,
        Some(paths) => {
            let mut out = Vec::new();
            for path in paths {
                if let Some(row) = rows.iter().find(|r| &r.path == path) {
                    out.push(row.clone());
                }
            }
            out
        }
    };
    if shown.is_empty() && fields.is_some() {
        // Every field the section named is absent from this record. Saying so is
        // better than an empty group that looks like a rendering bug.
        ui.weak("none of this section's fields are on this record");
        ui.add_space(4.0);
        return;
    }
    // The rows are walked in the section's own order — `TPLT` before `ACBS` puts
    // the picker above the checkboxes, which is where GECK puts it, and a
    // section that names them the other way round gets the other way round.
    //
    // A flags field is drawn **outside** the grid. The grid is two columns — a
    // label and a value — and a flags field's value is a list of checkboxes.
    // Left inside, it is one cell holding ten widgets: either they run off the
    // row with their labels truncated, or `end_row` before them puts them in a
    // cell the grid then stretches to the remaining height, scattering them
    // hundreds of points apart.
    //
    // So the rows are split into **runs** of one kind and each run gets the
    // container that fits it. Splitting into runs rather than grouping by kind
    // keeps the section's order: "all flags, then everything else" would
    // silently reorder every section that mixes the two, which is exactly what
    // the template group does.
    let mut runs: Vec<Vec<&nvre_edit::encode::Row>> = Vec::new();
    for row in &shown {
        let Some(current) = get_path(&view.value, &row.path) else {
            continue;
        };
        let flags = flag_bits(
            &session.schema,
            rec_sig,
            &view.sig,
            view.block.as_deref(),
            &row.path,
        );
        let is_flags = matches!(current, serde_json::Value::Object(_)) && !flags.is_empty();
        match runs.last_mut() {
            Some(run) if run_is_flags(run, view, session, rec_sig) == is_flags => run.push(row),
            _ => runs.push(vec![row]),
        }
    }
    for (ri, run) in runs.iter().enumerate() {
        if ri > 0 {
            ui.add_space(6.0);
        }
        if run_is_flags(run, view, session, rec_sig) {
            for row in run {
                let Some(current) = get_path(&view.value, &row.path) else {
                    continue;
                };
                field_editor(
                    ui,
                    session,
                    edit,
                    form_id,
                    view,
                    rec_sig,
                    &row.path,
                    current,
                    // The bits this section shows for this field, when the section
                    // split the flags word. `None` is the whole word.
                    flags_bits
                        .get(&format!("{}.{}", view.sig, row.path))
                        .map(Vec::as_slice),
                );
            }
        } else {
            egui::Grid::new(("fields", &view.sig, &view.block))
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    for row in run {
                        let Some(current) = get_path(&view.value, &row.path) else {
                            continue;
                        };
                        draw_field_row(
                            ui,
                            session,
                            edit,
                            form_id,
                            view,
                            rec_sig,
                            row,
                            current,
                            flags_bits,
                        );
                    }
                });
        }
    }
    ui.add_space(4.0);
}

/// Whether a run of rows is a run of flags fields.
///
/// The caller built each run from one kind, so the first row is the answer. A
/// flags word decodes to an object and every other field does not, which is the
/// same test [`field_editor`] makes to choose its widget.
fn render_single_section(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    form_id: u32,
    rec_sig: [u8; 4],
    views: &[nvre_edit::encode::SubView],
    edited: &[[u8; 4]],
    edit: &mut FieldEdit,
    si: usize,
    section: &nvre_esm::schema::SectionDef,
    bounds_out: Option<&std::sync::Mutex<Option<egui::Rect>>>,
) {
    let mut named: Vec<usize> = Vec::new();
    for tag in &section.subs {
        for (i, v) in views.iter().enumerate() {
            if &v.sig == tag && !named.contains(&i) {
                named.push(i);
            }
        }
    }
    let group = ui.group(|ui| {
        if !section.untitled {
            ui.strong(&section.title);
            ui.add_space(4.0);
        }
        match section.widget.as_deref() {
            Some(name) if name == "repeat" => {
                for tag_str in &section.subs {
                    let block = views
                        .iter()
                        .find(|v| v.sig == *tag_str)
                        .and_then(|v| v.block.as_deref());
                    repeat_widget(
                        ui,
                        session,
                        form_id,
                        &rec_sig,
                        &sig4(tag_str),
                        block,
                        edit,
                    );
                }
            }
            // The inventory list is one table per actor, not one per subrecord:
            // a `COED` belongs to the `CNTO` before it, so the two are zipped by
            // position rather than rendered as two unrelated lists.
            Some(name) if name == "inventory" => {
                inventory_widget(ui, session, form_id, &rec_sig, edit);
            }
            // The animation list is read-only and is not a subrecord list at
            // all: the base animations come from the model's folder in the
            // game's archives, and only the specialanims selection is read from
            // the record.
            Some(name) if name == "animations" => {
                // The table owns this tab's scroll. The page already wraps
                // itself in a scroll area, so a table that sized itself to its
                // content would nest a second scrollbar inside the first — two
                // bars for one tab, each scrolling part of the other.
                //
                // The table's own scroll area is given the viewport height
                // instead, so it is the only scrollbar and it resizes with the
                // window: shrink the panel and the table shrinks with it, grow
                // it and the table takes the room back.
                //
                // `available_height` inside a scroll area is the space left
                // after what has already been drawn — the header, the tab
                // strip — which is exactly the viewport the table should fill.
                let viewport = (ui.available_height() - 28.0).max(100.0);
                animations_widget(ui, session, form_id, edit, viewport);
            }
            Some(name) if name == "model_list" => {
                model_list_widget(ui, session, form_id, &rec_sig, edit);
            }
            Some(name) if name == "object_bounds" => {
                object_bounds_widget(ui, session, form_id, &rec_sig, edit);
            }
            Some(name) => {
                ui.colored_label(
                    egui::Color32::RED,
                    format!("the schema names widget {name:?}, which this build does not have"),
                );
            }
            None => {
                enum SecItem {
                    Row {
                        view_idx: usize,
                        row: nvre_edit::encode::Row,
                        is_flags: bool,
                    },
                    Absent([u8; 4]),
                }
                let mut items = Vec::new();
                for tag_str in &section.subs {
                    let tag = sig4(tag_str);
                    if let Some(i) = views.iter().position(|v| v.sig == *tag_str) {
                        let rows = nvre_edit::encode::rows(
                            &session.schema,
                            &rec_sig,
                            &tag,
                            views[i].block.as_deref(),
                            &views[i].value,
                        );
                        let shown: Vec<nvre_edit::encode::Row> = match section.fields.get(&views[i].sig) {
                            None => rows
                                .into_iter()
                                .filter(|r| r.path != "unused" && !r.path.ends_with(".unused"))
                                .collect(),
                            Some(paths) => {
                                let mut out = Vec::new();
                                for path in paths {
                                    if let Some(row) = rows.iter().find(|r| &r.path == path) {
                                        out.push(row.clone());
                                    }
                                }
                                out
                            }
                        };
                        for row in shown {
                            let is_flags = get_path(&views[i].value, &row.path).is_some_and(|current| {
                                let flags = flag_bits(
                                    &session.schema,
                                    &rec_sig,
                                    &views[i].sig,
                                    views[i].block.as_deref(),
                                    &row.path,
                                );
                                matches!(current, serde_json::Value::Object(_)) && !flags.is_empty()
                            });
                            items.push(SecItem::Row {
                                view_idx: i,
                                row,
                                is_flags,
                            });
                        }
                    } else {
                        items.push(SecItem::Absent(tag));
                    }
                }

                let mut runs: Vec<Vec<SecItem>> = Vec::new();
                for item in items {
                    let is_flags = match &item {
                        SecItem::Row { is_flags, .. } => *is_flags,
                        SecItem::Absent(_) => false,
                    };
                    match runs.last_mut() {
                        Some(run) => {
                            let run_flags = match &run[0] {
                                SecItem::Row { is_flags, .. } => *is_flags,
                                SecItem::Absent(_) => false,
                            };
                            if run_flags == is_flags {
                                run.push(item);
                            } else {
                                runs.push(vec![item]);
                            }
                        }
                        None => runs.push(vec![item]),
                    }
                }

                for (ri, run) in runs.iter().enumerate() {
                    if ri > 0 {
                        ui.add_space(6.0);
                    }
                    let run_flags = match &run[0] {
                        SecItem::Row { is_flags, .. } => *is_flags,
                        SecItem::Absent(_) => false,
                    };
                    if run_flags {
                        for item in run {
                            if let SecItem::Row { view_idx, row, .. } = item {
                                let view = &views[*view_idx];
                                let Some(current) = get_path(&view.value, &row.path) else { continue };
                                if row.path == "services" {
                                    ui.label("Buys / Sells and Services:");
                                }
                                field_editor(
                                    ui,
                                    session,
                                    edit,
                                    form_id,
                                    view,
                                    &rec_sig,
                                    &row.path,
                                    current,
                                    section.flags_bits
                                        .get(&format!("{}.{}", view.sig, row.path))
                                        .map(Vec::as_slice),
                                );
                            }
                        }
                    } else {
                        egui::Grid::new(("sec_grid", si, ri))
                            .num_columns(2)
                            .striped(true)
                            .show(ui, |ui| {
                                for item in run {
                                    match item {
                                        SecItem::Row { view_idx, row, .. } => {
                                            let view = &views[*view_idx];
                                            let Some(current) = get_path(&view.value, &row.path) else { continue };
                                            draw_field_row(
                                                ui,
                                                session,
                                                edit,
                                                form_id,
                                                view,
                                                &rec_sig,
                                                row,
                                                current,
                                                &section.flags_bits,
                                            );
                                        }
                                        SecItem::Absent(tag) => {
                                            let name = session
                                                .schema
                                                .lookup(&rec_sig, tag)
                                                .map(|d| d.name.clone())
                                                .unwrap_or_else(|| nvre_util::tag_str(tag));
                                            ui.label(&name);
                                            // An enum subrecord the record does not
                                            // carry still gets its dropdown: the
                                            // list of values comes from the
                                            // schema, so a creature with no
                                            // `EAMT` can be given an unarmed
                                            // attack animation without the
                                            // subrecord existing first.
                                            let variants = nvre_edit::encode::enum_values_at(
                                                &session.schema,
                                                &rec_sig,
                                                tag,
                                                None,
                                                "",
                                            );
                                            if !variants.is_empty() {
                                                let mut picked: i64 = -1;
                                                egui::ComboBox::from_id_salt(format!(
                                                    "absent_enum_{}",
                                                    nvre_util::tag_str(tag)
                                                ))
                                                .selected_text("NONE")
                                                .width(200.0)
                                                .show_ui(ui, |ui| {
                                                    for (v, n) in &variants {
                                                        ui.selectable_value(
                                                            &mut picked,
                                                            *v,
                                                            n,
                                                        );
                                                    }
                                                });
                                                if picked >= 0 {
                                                    let bytes = (picked as u16)
                                                        .to_le_bytes()
                                                        .to_vec();
                                                    session.doc.set_sub(form_id, tag, bytes);
                                                }
                                                ui.end_row();
                                                continue;
                                            }
                                            let targets = crate::object_editor::formid_targets_in(
                                                &session.schema,
                                                &rec_sig,
                                                tag,
                                                None,
                                                "",
                                            );
                                            let items = session.records_of_any(&targets);
                                            let mut chosen: Option<u32> = None;
                                            let none_label = if *tag == *b"ZNAM" { "Default" } else { "NONE" };
                                            crate::object_editor::record_dropdown_with_none(
                                                ui,
                                                &format!("absent_{}", nvre_util::tag_str(tag)),
                                                &items,
                                                &mut chosen,
                                                none_label,
                                            );
                                            if let Some(fid) = chosen {
                                                session.doc.set_sub(form_id, tag, fid.to_le_bytes().to_vec());
                                            }
                                            ui.end_row();
                                        }
                                    }
                                }
                            });
                    }
                }
            }
        }
    });
    if section.subs.iter().any(|t| t == "OBND") {
        if let Some(out) = bounds_out {
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(group.response.rect);
        }
    }
}

fn run_is_flags(
    run: &[&nvre_edit::encode::Row],
    view: &nvre_edit::encode::SubView,
    session: &crate::session::Session,
    rec_sig: &[u8; 4],
) -> bool {
    run.first().is_some_and(|row| {
        let flags = flag_bits(
            &session.schema,
            rec_sig,
            &view.sig,
            view.block.as_deref(),
            &row.path,
        );
        !flags.is_empty()
    })
}

/// One row of the field grid: the label, then the widget that fits the value.
///
/// Split out of [`subrecord_rows_on_fields`] because a section can switch
/// between the grid and a free-standing flags block part way through, and the
/// grid has to be open in both places without duplicating the row.
fn draw_field_row(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    edit: &mut FieldEdit,
    form_id: u32,
    view: &nvre_edit::encode::SubView,
    rec_sig: &[u8; 4],
    row: &nvre_edit::encode::Row,
    current: &serde_json::Value,
    flags_bits: &std::collections::BTreeMap<String, Vec<u32>>,
) {
    // The row label. A field inside a subrecord is named by its path; a
    // subrecord that *is* one field — `EDID`, `FULL`, `SCRI` — has an empty
    // path, and printing the word "value" for it said nothing a modder could
    // use. The subrecord's own name from the schema is the answer: "Editor ID",
    // "Name", "Script".
    let is_pc_level_mult = if view.sig == "ACBS" {
        view.value
            .get("flags")
            .and_then(|f| f.get("value"))
            .and_then(|v| v.as_u64())
            .map(|f| f & (1 << 7) != 0)
            .unwrap_or(false)
    } else {
        false
    };
    let label = if row.path.is_empty() {
        view.name.as_str()
    } else if let Some(attr) = row.path.strip_prefix("attributes.") {
        match attr {
            "strength" => "Strength",
            "perception" => "Perception",
            "endurance" => "Endurance",
            "charisma" => "Charisma",
            "intelligence" => "Intelligence",
            "agility" => "Agility",
            "luck" => "Luck",
            _ => attr,
        }
    } else {
        match row.path.as_str() {
            "karma" if view.sig == "ACBS" => "Alignment",
            "disposition_base" if view.sig == "ACBS" => "Disposition Base",
            "type" if view.sig == "DATA" => "Creature Type",
            "level_or_level_mult_raw" => {
                if is_pc_level_mult {
                    "Level Mult"
                } else {
                    "Level"
                }
            }
            "calc_min" => "Calc Min",
            "calc_max" => "Calc Max",
            "combat_skill" => "Combat Skill",
            "magic_skill" => "Magic Skill",
            "stealth_skill" => "Stealth Skill",
            "health" => "Health",
            "damage" => "Damage",
            "fatigue" => "Fatigue",
            "speed_multiplier" => "Speed Mult %",
            "barter_gold" => "Barter Gold",
            "aggression" => "Aggression",
            "confidence" => "Confidence",
            "energy_level" => "Energy Level",
            "responsibility" => "Responsibility",
            "mood" => "Mood",
            "assistance" => "Assistance",
            "aggro_radius_behavior" => "Aggro Radius Behavior",
            "aggro_radius" => "Aggro Radius",
            "teaches" => "Teaches",
            "maximum_training_level" => "Max Training Level",
            other => other,
        }
    };
    ui.label(label);
    let is_calc_min_max = view.sig == "ACBS" && (row.path == "calc_min" || row.path == "calc_max");
    let enabled = !is_calc_min_max || is_pc_level_mult;
    ui.add_enabled_ui(enabled, |ui| {
        if row.derived {
            ui.horizontal(|ui| {
                ui.monospace(field_text(current));
                ui.weak("(counted)");
            });
        } else {
            field_editor(
                ui,
                session,
                edit,
                form_id,
                view,
                rec_sig,
                &row.path,
                current,
                // The bits this section shows for this field, when the section split
                // the flags word. `None` is the whole word.
                flags_bits
                    .get(&format!("{}.{}", view.sig, row.path))
                    .map(Vec::as_slice),
            );
        }
    });
    ui.end_row();
}

/// Every subrecord of a record, as an editable grid.
///
/// This is the editor's game-neutral page. What it shows is the schema's answer:
/// each subrecord decoded through the block sequence the record was read in, and
/// inside it one row per field the layout declares. A field is editable exactly
/// when the encoder can write it back byte-exact, which [`sub_views`] decides by
/// re-encoding the subrecord's own bytes and comparing them — so a layout that
/// decodes but cannot be written is shown read-only with the reason, rather than
/// offering a widget that would corrupt the record.
///
/// Nothing here names a game, a record type, or a field. That is the point: a
/// record type no bespoke page has been written for is still editable, and a
/// game whose pages do not exist yet gets every field its schema describes. The
/// object editor falls back to this page when the schema's object-window table
/// names no page for a signature.
///
/// Edits go to the document as they are made, the way the reference property
/// grid's do: a field is one subrecord's worth of re-encoded bytes, which is
/// exactly what the document's `SetSub` edit holds.
pub fn schema_grid(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    form_id: u32,
    edit: &mut FieldEdit,
) {
    // The record as the mod will carry it: queued edits already applied, so the
    // grid shows what the field *is* rather than what the file said before.
    let Some(rec) = session.resolved_record(form_id) else {
        ui.colored_label(egui::Color32::RED, format!("{form_id:08X} not found"));
        return;
    };
    let rec_sig = rec.header.sig;
    let views =
        sub_views(&session.schema, &rec, session.record_is_localized(form_id)).unwrap_or_default();
    // Which subrecords have a queued edit, for the badge. The values come from
    // `resolved_record` above, which has the edits applied.
    let edited: Vec<[u8; 4]> = session
        .doc
        .edits_for(form_id)
        .unwrap_or_default()
        .iter()
        .filter_map(|e| match e {
            nvre_edit::document::Edit::SetSub { sig, .. } => Some(*sig),
            _ => None,
        })
        .collect();
    if views.is_empty() {
        ui.label("the schema describes no subrecords for this record");
        return;
    }
    if let Some(err) = &edit.error {
        ui.colored_label(egui::Color32::RED, err);
    }
    for s in &views {
        subrecord_rows(ui, session, form_id, &rec_sig, s, &edited, edit);
    }
}

/// A value as text, for the rows the grid shows but does not edit.
fn field_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Array(a) => format!("(list of {})", a.len()),
        serde_json::Value::Null => "(none)".to_string(),
        other => other.to_string(),
    }
}

/// The value at a dotted path inside a decoded subrecord.
///
/// The empty path is the whole value: a bare scalar subrecord has no inner
/// fields, so its own value *is* the field, and returning `None` for it would
/// leave every scalar subrecord — `XOWN`, `XSCL`, `XRDS` — with no row at all.
///
/// The walk itself lives in [`nvre_edit::encode::value_at`], so the grid reads a
/// path the same way the session writes one.
fn get_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    nvre_edit::encode::value_at(value, path)
}

/// A subrecord signature as the `[u8; 4]` the schema and the document want.
///
/// The property view carries it as a `String`, because that is what the grid
/// shows. A signature that is not four bytes cannot name a subrecord, so it
/// decodes to zeros and every lookup below misses — which is the correct
/// outcome for a value that should never occur.
fn sig4(s: &str) -> [u8; 4] {
    let b = s.as_bytes();
    [
        b.first().copied().unwrap_or(0),
        b.get(1).copied().unwrap_or(0),
        b.get(2).copied().unwrap_or(0),
        b.get(3).copied().unwrap_or(0),
    ]
}

/// One editable field: the widget that fits its value, wired to the session.
///
/// The widget is chosen by the JSON type the decoder produced, which is the
/// schema's own answer: a `f32` decodes to a number, an enum to a string, a
/// flags object to a map of bit → bool. Editing goes through
/// [`crate::session::Session::set_selected_field`], which re-encodes the whole
/// subrecord; a refusal is shown in place and nothing is written.
#[allow(clippy::too_many_arguments)]
/// Extract a FormID (as a non-zero u32) from either a JSON integer or a hex string ("0x00174363").
fn formid_from_json(v: &serde_json::Value) -> Option<u32> {
    match v {
        serde_json::Value::Number(n) => n.as_u64().map(|v| v as u32).filter(|fid| *fid != 0),
        serde_json::Value::String(s) => {
            let hex = s.strip_prefix("0x").unwrap_or(s.as_str());
            u32::from_str_radix(hex, 16).ok().filter(|fid| *fid != 0)
        }
        _ => None,
    }
}

fn field_editor(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    edit: &mut FieldEdit,
    form_id: u32,
    view: &nvre_edit::encode::SubView,
    rec_sig: &[u8; 4],
    path: &str,
    current: &serde_json::Value,
    // The bits of a flags field to draw, when the section split the word.
    //
    // `None` draws every named bit — the whole word, which is what every section
    // that is not splitting one wants. `Some` draws only the listed bits, in the
    // order listed, and a bit the section does not name is carried through
    // untouched on the write: the number is re-derived from the checkboxes that
    // *were* drawn, so a hidden bit survives on whichever tab owns it.
    bits_shown: Option<&[u32]>,
) {
    // A field of a read-only subrecord is not editable either: the bytes cannot
    // be written back, so offering a widget would only mislead.
    if !view.writable {
        ui.colored_label(egui::Color32::YELLOW, "read-only");
        return;
    }
    let sig = sig4(&view.sig);
    let mut changed: Option<serde_json::Value> = None;
    // A FormID field gets a picker, filtered to the record types the schema says
    // it may point at. The check comes first because a FormID decodes to a
    // number and would otherwise be offered a bare spinner.
    let targets = crate::object_editor::formid_targets_in(
        &session.schema,
        rec_sig,
        &sig,
        view.block.as_deref(),
        path,
    );
    let is_formid = nvre_edit::encode::is_formid_field(
        &session.schema,
        rec_sig,
        &sig,
        view.block.as_deref(),
        path,
    );
    if is_formid {
        let mut opt = formid_from_json(current);
        let items = session.records_of_any(&targets);
        ui.horizontal(|ui| {
            let none_label = if view.sig == "ZNAM" { "Default" } else { "NONE" };
            crate::object_editor::record_dropdown_with_none(
                ui,
                &format!("{}_{}", view.sig, path),
                &items,
                &mut opt,
                none_label,
            );
            // Which record types the dropdown is filtered to. Shown only when
            // there is a filter: "LVLI, SPEL" tells a modder what may go here,
            // and "any record" tells them nothing they cannot see from the
            // dropdown listing every record in the game.
            if !targets.is_empty() {
                ui.weak(targets.join(", "));
            }
        });
        if opt != formid_from_json(current) {
            changed = Some(match opt {
                Some(fid) => serde_json::json!(fid),
                None => serde_json::json!(0),
            });
        }
        if let Some(value) = changed {
            edit.last = Some((sig, path.to_string()));
            match session.set_object_field(form_id, &sig, path, value) {
                Ok(()) => edit.error = None,
                Err(e) => edit.error = Some(e),
            }
        }
        return;
    }
    // An enum decodes to `{"value": n, "name": "…"}` — one number and the name
    // of that number. A dropdown is the right widget: the names are the schema's
    // vocabulary, and a modder picking "Very Hard" should not have to know it is
    // 3. The table is read before the type switch because the decoded shape is an
    // object, which the object arm would otherwise render as `(object)`.
    let variants = nvre_edit::encode::enum_values_at(
        &session.schema,
        rec_sig,
        &sig,
        view.block.as_deref(),
        path,
    );
    // Alignment dropdown for ACBS karma (Very Good, Good, Neutral, Evil, Very Evil).
    // GECK presents this as an "Alignment" dropdown rather than a raw float input.
    if sig == *b"ACBS" && path == "level_or_level_mult_raw" {
        let is_pc_level_mult = view
            .value
            .get("flags")
            .and_then(|f| f.get("value"))
            .and_then(|v| v.as_u64())
            .map(|f| f & (1 << 7) != 0)
            .unwrap_or(false);
        if is_pc_level_mult {
            let raw = current.as_u64().unwrap_or(1000);
            let mut mult = if raw == 0 || raw == 1 {
                1.0
            } else {
                (raw as f64 / 1000.0).max(0.001)
            };
            let drag = egui::DragValue::new(&mut mult)
                .speed(0.05)
                .range(0.001..=65.0)
                .custom_formatter(|n, _| format!("{:.4}", n));
            if ui.add_sized([70.0, 18.0], drag).changed() {
                let next_raw = (mult * 1000.0).round() as u64;
                edit.last = Some((sig, path.to_string()));
                match session.set_object_field(form_id, &sig, path, serde_json::json!(next_raw)) {
                    Ok(()) => edit.error = None,
                    Err(e) => edit.error = Some(e),
                }
            }
            return;
        }
    }

    if sig == *b"ACBS" && path == "karma" {
        let current_karma = current.as_f64().unwrap_or(0.0) as f32;
        let alignments: [(&str, f32); 5] = [
            ("Very Good", 900.0),
            ("Good", 500.0),
            ("Neutral", 0.0),
            ("Evil", -500.0),
            ("Very Evil", -900.0),
        ];
        let cur_align = if current_karma >= 750.0 {
            "Very Good"
        } else if current_karma >= 250.0 {
            "Good"
        } else if current_karma <= -750.0 {
            "Very Evil"
        } else if current_karma <= -250.0 {
            "Evil"
        } else {
            "Neutral"
        };
        let mut picked_karma = current_karma;
        egui::ComboBox::from_id_salt(format!("align_{}_{}", view.sig, path))
            .selected_text(cur_align)
            .width(160.0)
            .show_ui(ui, |ui| {
                for (name, val) in alignments {
                    let is_sel = cur_align == name;
                    if ui.selectable_label(is_sel, name).clicked() {
                        picked_karma = val;
                    }
                }
            });
        if (picked_karma - current_karma).abs() > f32::EPSILON {
            edit.last = Some((sig, path.to_string()));
            match session.set_object_field(form_id, &sig, path, serde_json::json!(picked_karma)) {
                Ok(()) => edit.error = None,
                Err(e) => edit.error = Some(e),
            }
        }
        return;
    }

    if !variants.is_empty() {
        if let Some(raw) = current.get("value").and_then(|v| v.as_i64()) {
            let is_bool_enum = variants.len() == 2
                && variants.iter().any(|(v, n)| *v == 0 && n == "False")
                && variants.iter().any(|(v, n)| *v == 1 && n == "True");
            if is_bool_enum {
                let mut checked = raw != 0;
                if ui.checkbox(&mut checked, "").changed() {
                    let picked = if checked { 1 } else { 0 };
                    let mut obj = serde_json::Map::new();
                    obj.insert("value".into(), serde_json::json!(picked));
                    obj.insert(
                        "name".into(),
                        serde_json::json!(if checked { "True" } else { "False" }),
                    );
                    edit.last = Some((sig, path.to_string()));
                    match session.set_object_field(
                        form_id,
                        &sig,
                        path,
                        serde_json::Value::Object(obj),
                    ) {
                        Ok(()) => edit.error = None,
                        Err(e) => edit.error = Some(e),
                    }
                }
                return;
            }
            let cur_name = variants
                .iter()
                .find(|(v, _)| *v == raw)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| raw.to_string());
            let mut picked = raw;
            egui::ComboBox::from_id_salt(format!("enum_{}_{}", view.sig, path))
                .selected_text(cur_name)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for (value, name) in &variants {
                        ui.selectable_value(&mut picked, *value, name);
                    }
                });
            if picked != raw {
                // The decoder's shape is `{value, name}`; the name is derived from
                // the value by the encoder, so only the value is written and the
                // decoder fills the name back in.
                let mut obj = serde_json::Map::new();
                obj.insert("value".into(), serde_json::json!(picked));
                if let Some((_, name)) = variants.iter().find(|(v, _)| *v == picked) {
                    obj.insert("name".into(), serde_json::json!(name));
                }
                edit.last = Some((sig, path.to_string()));
                match session.set_object_field(form_id, &sig, path, serde_json::Value::Object(obj))
                {
                    Ok(()) => edit.error = None,
                    Err(e) => edit.error = Some(e),
                }
            }
        } else {
            ui.monospace("(enum with no value)");
        }
        return;
    }
    match current {
        serde_json::Value::Number(n) => {
            let mut v = n.as_f64().unwrap_or(0.0);
            let is_int = n.is_i64() || n.is_u64();
            let drag = egui::DragValue::new(&mut v).speed(if is_int { 1.0 } else { 0.1 });
            if ui.add_sized([70.0, 18.0], drag).changed() {
                changed = Some(if is_int {
                    serde_json::json!(v.round() as i64)
                } else {
                    serde_json::json!(v)
                });
            }
        }
        serde_json::Value::String(s) => {
            let mut text = s.clone();
            let is_model = view.sig == "MODL";
            let width = if is_model { 260.0 } else { 120.0 };
            ui.horizontal(|ui| {
                if ui
                    .add(egui::TextEdit::singleline(&mut text).desired_width(width))
                    .changed()
                {
                    changed = Some(serde_json::json!(text));
                }
                if is_model {
                    if ui
                        .button("Edit...")
                        .on_hover_text("Browse for a .nif model file")
                        .clicked()
                    {
                        let meshes_dir = session.data_dir.join("meshes");
                        let mut dialog = rfd::FileDialog::new()
                            .set_title("Select Model File")
                            .add_filter("NIF Model (*.nif)", &["nif", "NIF"]);
                        if meshes_dir.is_dir() {
                            dialog = dialog.set_directory(&meshes_dir);
                        } else if session.data_dir.is_dir() {
                            dialog = dialog.set_directory(&session.data_dir);
                        }
                        if let Some(chosen) = dialog.pick_file() {
                            let rel = if let Ok(rel_path) = chosen.strip_prefix(&session.data_dir) {
                                rel_path.to_string_lossy().to_string()
                            } else {
                                chosen.to_string_lossy().to_string()
                            };
                            let norm = nvre_vfs::normalize(&rel);
                            text = norm.clone();
                            changed = Some(serde_json::json!(norm));
                        }
                    }
                }
                if is_model && !text.is_empty() {
                    let norm = nvre_vfs::normalize(&text);
                    if session.vfs_exists(&norm) {
                        ui.colored_label(egui::Color32::from_rgb(100, 200, 120), "✔ Found");
                    } else {
                        ui.colored_label(egui::Color32::from_rgb(220, 100, 100), "⚠ Missing");
                    }
                }
            });
        }
        // A flags field decodes to `{"value": n, "set": [names], "unknown_bits":
        // hex}`: the raw number, the names of the bits that are on, and any bit
        // the schema does not name. Each named bit becomes a checkbox, and the
        // bits the schema does not name are carried through untouched — a field
        // the editor cannot explain still round-trips byte-exact.
        serde_json::Value::Object(o) => {
            let bits = flag_bits(
                &session.schema,
                rec_sig,
                &view.sig,
                view.block.as_deref(),
                path,
            );
            if !bits.is_empty() {
                let raw = o.get("value").and_then(|v| v.as_u64()).unwrap_or(0);
                let unknown = o
                    .get("unknown_bits")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let mut next = raw;
                let mut touched = false;
                // The bits this section draws. A section that split the word
                // names its own subset, in its own order; every other section
                // draws the whole word in bit order.
                let shown: Vec<(u32, String)> = match bits_shown {
                    Some(want) => want
                        .iter()
                        .filter_map(|b| bits.iter().find(|(bit, _)| bit == b).cloned())
                        .collect(),
                    None => bits.clone(),
                };
                // One checkbox per bit, wrapping rather than running off the
                // row: a creature's `ACBS.flags` is ten long names, and a row
                // that cannot hold them all truncates every label instead of
                // showing them. `horizontal_wrapped` starts a new line when the
                // next checkbox would not fit.
                ui.horizontal_wrapped(|ui| {
                    for (bit, name) in &shown {
                        let on = next & (1 << bit) != 0;
                        let mut want = on;
                        if ui.checkbox(&mut want, name).changed() {
                            touched = true;
                            if want {
                                next |= 1 << bit;
                            } else {
                                next &= !(1 << bit);
                            }
                        }
                    }
                });
                // The named bits are re-derived from the number, so the set list
                // cannot disagree with it. A bit that is on but unnamed is not
                // lost: it is preserved in `unknown_bits`.
                if touched {
                    let set: Vec<&str> = bits
                        .iter()
                        .filter(|(bit, _)| next & (1 << bit) != 0)
                        .map(|(_, name)| name.as_str())
                        .collect();
                    let named_mask: u64 = bits.iter().fold(0, |m, (bit, _)| m | (1 << bit));
                    let unexplained = next & !named_mask;
                    let mut obj = serde_json::Map::new();
                    obj.insert("value".into(), serde_json::json!(next));
                    obj.insert("set".into(), serde_json::json!(set));
                    if unexplained != 0 {
                        obj.insert(
                            "unknown_bits".into(),
                            serde_json::json!(format!("{unexplained:X}")),
                        );
                    } else if unknown.is_some() {
                        // Every bit is now explained: the unknown-bits entry would
                        // be stale, so it is dropped rather than re-encoded.
                        obj.insert("unknown_bits".into(), serde_json::json!("0x0"));
                    }
                    changed = Some(serde_json::Value::Object(obj));
                }
            } else {
                ui.monospace("(object)");
            }
        }
        serde_json::Value::Bool(b) => {
            let mut next = *b;
            if ui.checkbox(&mut next, "").changed() {
                changed = Some(serde_json::json!(next));
            }
        }
        // A list: one row per element, each editable as a whole value, plus a
        // way to add and remove one. The edit is the whole array, because an
        // array's length is part of its encoding — writing one element in place
        // would have to resize the subrecord without telling the encoder.
        serde_json::Value::Array(items) => {
            let elem = nvre_edit::encode::array_elem_at(
                &session.schema,
                rec_sig,
                &sig,
                view.block.as_deref(),
                path,
            );
            let mut next: Option<Vec<serde_json::Value>> = None;
            ui.vertical(|ui| {
                let mut remove: Option<usize> = None;
                for (i, item) in items.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("[{i}]"));
                        match item {
                            // A scalar element gets the same widget a scalar
                            // field does, at the element's own path.
                            serde_json::Value::Number(n) => {
                                let mut v = n.as_f64().unwrap_or(0.0);
                                let is_int = n.is_i64() || n.is_u64();
                                let drag = egui::DragValue::new(&mut v).speed(if is_int {
                                    1.0
                                } else {
                                    0.1
                                });
                                if ui.add(drag).changed() {
                                    let mut list = items.clone();
                                    list[i] = if is_int {
                                        serde_json::json!(v.round() as i64)
                                    } else {
                                        serde_json::json!(v)
                                    };
                                    next = Some(list);
                                }
                            }
                            serde_json::Value::Bool(b) => {
                                let mut v = *b;
                                if ui.checkbox(&mut v, "").changed() {
                                    let mut list = items.clone();
                                    list[i] = serde_json::json!(v);
                                    next = Some(list);
                                }
                            }
                            // A struct element is edited field by field, each one
                            // addressed as `<array>.<index>.<field>` — which is
                            // what `set_path` walks, and what `rows()` produces
                            // for an array the grid would otherwise flatten.
                            serde_json::Value::Object(fields) => {
                                for (name, value) in fields {
                                    ui.label(name);
                                    let mut v = value.as_f64().unwrap_or(0.0);
                                    if value.is_number() {
                                        let is_int = value.is_i64() || value.is_u64();
                                        let drag = egui::DragValue::new(&mut v).speed(if is_int {
                                            1.0
                                        } else {
                                            0.1
                                        });
                                        if ui.add(drag).changed() {
                                            let mut list = items.clone();
                                            let put = if is_int {
                                                serde_json::json!(v.round() as i64)
                                            } else {
                                                serde_json::json!(v)
                                            };
                                            if let Some(serde_json::Value::Object(o)) =
                                                list.get_mut(i)
                                            {
                                                o.insert(name.clone(), put);
                                            }
                                            // The whole array is written, through
                                            // the same path the grid addresses it
                                            // by, so the encoder sees one value.
                                            next = Some(list);
                                        }
                                    } else {
                                        ui.monospace(value.to_string());
                                    }
                                }
                            }
                            other => {
                                ui.monospace(other.to_string());
                            }
                        }
                        if ui.small_button("remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    let mut list = items.clone();
                    list.remove(i);
                    next = Some(list);
                }
                let count = items.len();
                ui.horizontal(|ui| {
                    ui.weak(format!("{count} element(s)"));
                    if ui.small_button("add").clicked() {
                        // A new element is the layout's zero: the encoder rejects
                        // nothing that encodes, and an empty struct of the right
                        // shape is the honest starting point.
                        let mut list = items.clone();
                        list.push(match elem.as_ref() {
                            Some(nvre_esm::TypeDef::Struct(_)) | None => {
                                serde_json::Value::Object(Default::default())
                            }
                            Some(nvre_esm::TypeDef::Prim(nvre_esm::schema::Prim::F32)) => {
                                serde_json::json!(0.0)
                            }
                            Some(nvre_esm::TypeDef::Prim(nvre_esm::schema::Prim::FormId)) => {
                                serde_json::json!(0)
                            }
                            Some(_) => serde_json::json!(0),
                        });
                        next = Some(list);
                    }
                });
            });
            if let Some(list) = next {
                changed = Some(serde_json::Value::Array(list));
            }
        }
        serde_json::Value::Null => {
            ui.monospace("(none)");
        }
    }
    if let Some(value) = changed {
        edit.last = Some((sig, path.to_string()));
        match session.set_object_field(form_id, &sig, path, value) {
            Ok(()) => edit.error = None,
            Err(e) => edit.error = Some(e),
        }
    }
}

/// Every widget name a section may use.
///
/// A section's `widget` is a name looked up here. Keeping the list in one place is
/// what lets the schema's own tests assert that every name a game's table uses is
/// one this build has — a typo in a JSON file would otherwise render as the
/// default and look like it worked.
pub fn widget_names() -> Vec<&'static str> {
    vec!["repeat"]
}

/// A section of repeated subrecords: one row per occurrence, with add and remove.
///
/// The widget a leveled list, a container's contents or a spell list needs. The
/// grid shows one row per *field*, which is right for a struct but wrong for a
/// record that repeats a subrecord: an `LVLI` with twelve entries is twelve
/// `LVLO` subrecords, and what a modder does with them is add one, remove one, or
/// change a row's numbers.
///
/// The edit is `Document::set_sub_list`, which replaces every subrecord with the
/// signature in one queued edit. That is the honest unit: inserting one row
/// changes the record's subrecord sequence, not a subrecord's bytes, so an edit
/// that could only set bytes could not add a row at all.
///
/// Removing the last row removes the subrecord, which is what `set_sub_list` with
/// an empty list means — a record whose list is empty has no `LVLO`, and writing
/// a zero-length one would be a different record.
pub fn repeat_widget(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    form_id: u32,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    block: Option<&str>,
    edit: &mut FieldEdit,
) {
    let Some(rec) = session.resolved_record(form_id) else {
        ui.colored_label(egui::Color32::RED, format!("{form_id:08X} not found"));
        return;
    };
    let Ok(subs) = rec.subrecords() else {
        return;
    };
    // Every occurrence of the signature, as decoded values plus their bytes.
    let mut rows: Vec<(Vec<u8>, serde_json::Value)> = Vec::new();
    for s in subs.iter().filter(|s| &s.sig == sub_sig) {
        let v = nvre_edit::encode::decoded_in_block_localized(
            &session.schema,
            rec_sig,
            s,
            block,
            session.record_is_localized(form_id),
        );
        rows.push((s.data.clone(), v.unwrap_or(serde_json::Value::Null)));
    }

    let mut want: Option<Vec<Vec<u8>>> = None;
    let mut remove_at: Option<usize> = None;
    let mut move_up: Option<usize> = None;
    let mut move_down: Option<usize> = None;
    ui.vertical(|ui| {
        let is_snam = sub_sig == b"SNAM";
        let is_pkid = sub_sig == b"PKID";
        let schema_keys: Option<Vec<String>> = session
            .schema
            .lookup_in_block(rec_sig, sub_sig, block)
            .and_then(|def| match session.schema.resolve_def(&def.ty) {
                nvre_esm::typedef::TypeDef::Struct(fields) => Some(
                    fields
                        .iter()
                        .map(|f| f.name.clone())
                        .filter(|n| n != "unused")
                        .collect(),
                ),
                _ => None,
            });
        let visible_keys: Option<Vec<String>> = schema_keys.or_else(|| {
            rows.first().and_then(|(_, v)| {
                v.as_object().map(|obj| {
                    obj.keys()
                        .filter(|k| *k != "unused")
                        .cloned()
                        .collect()
                })
            })
        });

        if let Some(visible_keys) = visible_keys {
            // Render as a clean table with column headers rather than repeating
            // field names on every single row.
            egui::Grid::new(("repeat_table", sub_sig))
                .num_columns(visible_keys.len() + 1)
                .striped(true)
                .show(ui, |ui| {
                    ui.label(""); // blank for remove button
                    for key in &visible_keys {
                        let header = match key.as_str() {
                            "faction" => "Faction",
                            "rank" => "Rank",
                            other => other,
                        };
                        ui.strong(header);
                    }
                    ui.end_row();

                    if rows.is_empty() {
                        ui.label("");
                        ui.weak(if is_snam {
                            "No factions assigned"
                        } else {
                            "(no rows)"
                        });
                        ui.label("");
                        ui.end_row();
                    } else {
                        for (i, (bytes, value)) in rows.iter().enumerate() {
                            ui.push_id(i, |ui| {
                                if ui
                                    .small_button("✖")
                                    .on_hover_text("remove this row")
                                    .clicked()
                                {
                                    remove_at = Some(i);
                                }
                            });
                            if let serde_json::Value::Object(fields) = value {
                                let mut next = value.clone();
                                let mut touched = false;
                                for (name, fv) in fields {
                                    if name == "unused" {
                                        continue;
                                    }
                                    let path = if name.is_empty() {
                                        String::new()
                                    } else {
                                        name.clone()
                                    };
                                    if inline_value(
                                        ui, session, rec_sig, sub_sig, block, i, &path, fv, &mut next,
                                    ) {
                                        touched = true;
                                    }
                                }
                                if touched {
                                    if let Ok(bytes) = nvre_edit::encode::encode_sub_len_in_block(
                                        &session.schema,
                                        rec_sig,
                                        sub_sig,
                                        &next,
                                        Some(bytes.len()),
                                        block,
                                    ) {
                                        let mut list: Vec<Vec<u8>> =
                                            rows.iter().map(|(b, _)| b.clone()).collect();
                                        list[i] = bytes;
                                        want = Some(list);
                                    }
                                }
                            }
                            ui.end_row();
                        }
                    }
                });
        } else if is_pkid {
            egui::Grid::new(("repeat_table", sub_sig))
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    ui.label(""); // Actions
                    ui.strong("Priority");
                    ui.strong("Package");
                    ui.end_row();

                    if rows.is_empty() {
                        ui.label("");
                        ui.label("");
                        ui.weak("No AI packages assigned");
                        ui.end_row();
                    } else {
                        let len = rows.len();
                        for (i, (bytes, value)) in rows.iter().enumerate() {
                            ui.push_id(i, |ui| {
                                ui.horizontal(|ui| {
                                    if ui
                                        .small_button("✖")
                                        .on_hover_text("remove this package")
                                        .clicked()
                                    {
                                        remove_at = Some(i);
                                    }
                                    let up_btn = ui.add_enabled(i > 0, egui::Button::new("Up").small());
                                    if up_btn.on_hover_text("move up (higher priority)").clicked() {
                                        move_up = Some(i);
                                    }
                                    let down_btn = ui.add_enabled(i + 1 < len, egui::Button::new("Down").small());
                                    if down_btn.on_hover_text("move down (lower priority)").clicked() {
                                        move_down = Some(i);
                                    }
                                });
                            });
                            ui.monospace(format!("[{i}]"));
                            let mut next = value.clone();
                            if inline_value(ui, session, rec_sig, sub_sig, block, i, "", value, &mut next) {
                                if let Ok(bytes) = nvre_edit::encode::encode_sub_len_in_block(
                                    &session.schema,
                                    rec_sig,
                                    sub_sig,
                                    &next,
                                    Some(bytes.len()),
                                    block,
                                ) {
                                    let mut list: Vec<Vec<u8>> =
                                        rows.iter().map(|(b, _)| b.clone()).collect();
                                    list[i] = bytes;
                                    want = Some(list);
                                }
                            }
                            ui.end_row();
                        }
                    }
                });
        } else {
            // Bare rows: a list of one value each, with no struct to give it a
            // name — `SPLO` is a FormID and nothing else, and a form list is
            // nothing but FormIDs. A real table, so the rows virtualize and the
            // header says what the column holds.
            let header = match sub_sig {
                b"SPLO" => "Actor Effect",
                _ => "Value",
            };
            let empty_note = match sub_sig {
                b"SPLO" => "No actor effects",
                _ => "(no rows)",
            };
            egui_extras::TableBuilder::new(ui)
                .striped(true)
                .id_salt(("bare_table", sub_sig))
                .column(egui_extras::Column::initial(88.0).at_least(88.0).clip(true))
                .column(egui_extras::Column::remainder().at_least(120.0).clip(true))
                .header(18.0, |mut header_ui| {
                    header_ui.col(|ui| {
                        ui.strong("");
                    });
                    header_ui.col(|ui| {
                        ui.strong(header);
                    });
                })
                .body(|body| {
                    body.rows(20.0, rows.len(), |mut row_ui| {
                        let i = row_ui.index();
                        let Some((bytes, value)) = rows.get(i) else {
                            return;
                        };
                        row_ui.col(|ui| {
                            ui.push_id(i, |ui| {
                                ui.horizontal(|ui| {
                                    if ui
                                        .small_button("✖")
                                        .on_hover_text("remove this row")
                                        .clicked()
                                    {
                                        remove_at = Some(i);
                                    }
                                    let up = ui.add_enabled(i > 0, egui::Button::new("Up").small());
                                    if up.on_hover_text("move up").clicked() {
                                        move_up = Some(i);
                                    }
                                    let down = ui.add_enabled(
                                        i + 1 < rows.len(),
                                        egui::Button::new("Down").small(),
                                    );
                                    if down.on_hover_text("move down").clicked() {
                                        move_down = Some(i);
                                    }
                                });
                            });
                        });
                        row_ui.col(|ui| {
                            let mut next = value.clone();
                            if inline_value(
                                ui,
                                session,
                                rec_sig,
                                sub_sig,
                                block,
                                i,
                                "",
                                value,
                                &mut next,
                            ) {
                                if let Ok(bytes) = nvre_edit::encode::encode_sub_len_in_block(
                                    &session.schema,
                                    rec_sig,
                                    sub_sig,
                                    &next,
                                    Some(bytes.len()),
                                    block,
                                ) {
                                    let mut list: Vec<Vec<u8>> =
                                        rows.iter().map(|(b, _)| b.clone()).collect();
                                    list[i] = bytes;
                                    want = Some(list);
                                }
                            }
                        });
                    });
                });
            if rows.is_empty() {
                ui.add_space(4.0);
                ui.weak(empty_note);
            }
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let add_label = if is_snam {
                "+ Add Faction"
            } else if is_pkid {
                "+ Add Package"
            } else if sub_sig == b"SPLO" {
                "+ Add Actor Effect"
            } else {
                "+ Add Row"
            };
            if ui.button(add_label).clicked() {
                // A new row is the layout's zero, encoded — the same rule the
                // list widget uses for a new element.
                let zero = match session.schema.lookup_in_block(rec_sig, sub_sig, block) {
                    Some(def) => zero_value(&session.schema.resolve_def(&def.ty)),
                    None => serde_json::Value::Null,
                };
                if let Ok(bytes) = nvre_edit::encode::encode_sub_len_in_block(
                    &session.schema,
                    rec_sig,
                    sub_sig,
                    &zero,
                    None,
                    block,
                ) {
                    let mut list: Vec<Vec<u8>> = rows.iter().map(|(b, _)| b.clone()).collect();
                    list.push(bytes);
                    want = Some(list);
                }
            }
        });
    });
    if let Some(i) = remove_at {
        let mut list: Vec<Vec<u8>> = rows.iter().map(|(b, _)| b.clone()).collect();
        list.remove(i);
        want = Some(list);
    }
    if let Some(i) = move_up {
        if i > 0 && i < rows.len() {
            let mut list: Vec<Vec<u8>> = rows.iter().map(|(b, _)| b.clone()).collect();
            list.swap(i, i - 1);
            want = Some(list);
        }
    }
    if let Some(i) = move_down {
        if i + 1 < rows.len() {
            let mut list: Vec<Vec<u8>> = rows.iter().map(|(b, _)| b.clone()).collect();
            list.swap(i, i + 1);
            want = Some(list);
        }
    }
    if let Some(list) = want {
        session.doc.set_sub_list(form_id, sub_sig, list);
        session.stale = true;
        edit.error = None;
    }
}

/// The animation list: every `.kf` the actor can play, in the two groups the
/// engine gives them.
///
/// The **base** animations come from the folder the model's skeleton sits in.
/// The engine discovers them at runtime by scanning that folder, so they are
/// never written to the record and are always on — hence the checkboxes are
/// drawn disabled, not absent: a modder looking at a creature's animation list
/// needs to see that `mtidle.kf` is there and cannot be turned off.
///
/// The **special** overrides come from the `specialanims` subfolder. These *are*
/// written to the record — `KFFZ` holds exactly the ones selected — so they are
/// read back from the bytes, and a creature with no `KFFZ` shows none selected.
/// Ticking one rewrites `KFFZ`; unticking the last one drops the subrecord,
/// which is the same state the record would have had if it had never carried it.
///
/// There is deliberately no add or remove. The list is a *selection* from a
/// folder the engine owns, so an arbitrary string would let a record name a file
/// that does not exist; adding an animation means putting a `.kf` in the folder,
/// which is not something the record editor can do.
pub fn animations_widget(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    form_id: u32,
    edit: &mut crate::panels::FieldEdit,
    viewport: f32,
) {
    let Some(rec) = session.resolved_record(form_id) else {
        ui.colored_label(egui::Color32::RED, format!("{form_id:08X} not found"));
        return;
    };
    let rec_sig = rec.header.sig;
    // The record's own selection. `KFFZ` is **one** subrecord holding every
    // selected file name, NUL-separated, with a trailing NUL — not one
    // subrecord per name.
    let selected: Vec<String> = rec
        .subrecords()
        .map(|subs| {
            subs.iter()
                .filter(|s| &s.sig == b"KFFZ")
                .flat_map(|s| {
                    String::from_utf8_lossy(&s.data)
                        .split('\0')
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .filter(|n| !n.is_empty())
                .map(|n| n.to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    // The actor inherits its animations from its template, in which case this
    // list is not what the game uses and editing it does nothing.
    let inherits = rec
        .subrecords()
        .ok()
        .and_then(|subs| {
            subs.iter()
                .find(|s| &s.sig == b"ACBS")
                .map(|s| s.data.len() >= 8 && (s.data[6] & 0x40) != 0)
        })
        .unwrap_or(false);

    let anims = session.animation_set_for(form_id);
    let base = anims.as_ref().map(|a| a.base.clone()).unwrap_or_default();
    let special = anims.as_ref().map(|a| a.special.clone()).unwrap_or_default();

    if inherits {
        ui.label(
            egui::RichText::new(
                "This actor inherits its animations from its template (ACBS: Use Model/Animation), so this list is not used in game.",
            )
            .color(egui::Color32::YELLOW),
        );
        ui.add_space(4.0);
    }

    if base.is_empty() && special.is_empty() {
        ui.weak("no animations found for this actor's model");
        return;
    }

    // One table for both groups: the base rows are checked and disabled, the
    // special rows follow the record. A single table keeps the two groups in
    // one scrollable list, the way GECK shows them.
    //
    // The table owns this tab's scroll. `max_scroll_height` is the height at
    // which the table stops growing and starts scrolling instead, so setting it
    // to the viewport makes the table exactly as tall as the window lets it be:
    // it fills the space when there are few rows and scrolls when there are
    // many, and it resizes with the window because `viewport` is re-read every
    // frame.
    //
    // `auto_shrink([false, true])` is what keeps the table from also drawing
    // blank space below itself when the content is short — the blank goes
    // inside the table's own scroll area instead, which is the same thing the
    // other tables in the editor do.
    let total = base.len() + special.len();
    let mut toggled: Option<String> = None;
    egui_extras::TableBuilder::new(ui)
        .striped(true)
        .id_salt(("animations_table", rec_sig))
        .vscroll(true)
        .max_scroll_height(viewport)
        .auto_shrink([false, false])
        .column(egui_extras::Column::initial(70.0).at_least(70.0))
        .column(egui_extras::Column::initial(70.0).at_least(70.0))
        .column(egui_extras::Column::remainder().at_least(200.0).clip(true))
        .header(18.0, |mut header| {
            header.col(|ui| {
                ui.strong("");
            });
            header.col(|ui| {
                ui.strong("In use");
            });
            header.col(|ui| {
                ui.strong("Animation");
            });
        })
        .body(|body| {
            body.rows(20.0, total, |mut row_ui| {
                let i = row_ui.index();
                let (path, is_special) = match i < base.len() {
                    true => (&base[i], false),
                    false => (&special[i - base.len()], true),
                };
                row_ui.col(|ui| {
                    ui.weak(if is_special { "special" } else { "" });
                });
                row_ui.col(|ui| {
                    // Base animations are always on and cannot be turned off.
                    // Special ones follow what the record selected.
                    if is_special {
                        let name = path
                            .rsplit('\\')
                            .next()
                            .unwrap_or(path.as_str())
                            .to_lowercase();
                        let mut on = selected.iter().any(|s| s == &name);
                        if ui.checkbox(&mut on, "").changed() {
                            toggled = Some(name);
                        }
                    } else {
                        let mut on_mut = true;
                        ui.add_enabled(false, egui::Checkbox::new(&mut on_mut, ""));
                    }
                });
                row_ui.col(|ui| {
                    ui.label(path.rsplit('\\').next().unwrap_or(path.as_str()))
                        .on_hover_text(path.as_str());
                });
            });
        });

    // A special override was ticked or unticked: rewrite `KFFZ` as the whole
    // selection, so the subrecord holds exactly what the table shows. The name
    // is stored as the record stored it — the file's name, not its folder —
    // which is what the engine reads back.
    if let Some(name) = toggled {
        let mut names: Vec<String> = selected
            .iter()
            .filter(|n| n.as_str() != name)
            .cloned()
            .collect();
        if !selected.iter().any(|s| s == &name) {
            names.push(name);
        }
        let arr = serde_json::Value::Array(
            names
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        );
        match nvre_edit::encode::encode_sub(&session.schema, &rec_sig, b"KFFZ", &arr) {
            // An empty selection is not an empty `KFFZ`: the subrecord is
            // dropped, which is the state the record would have had if it had
            // never carried one. The encoder writes the closing NUL only for a
            // non-empty list, so this is the one case the bytes would differ.
            Ok(bytes) if !arr.as_array().is_some_and(|a| a.is_empty()) => {
                session.doc.set_sub(form_id, b"KFFZ", bytes);
                session.stale = true;
                edit.error = None;
            }
            _ => {
                session.doc.remove_sub(form_id, b"KFFZ");
                session.stale = true;
                edit.error = None;
            }
        }
    }

    ui.add_space(4.0);
    ui.weak(format!(
        "{} base animation(s), always on; {} special override(s), {} selected by this record",
        base.len(),
        special.len(),
        selected.len()
    ));
}

/// The Model List (`NIFZ`) table: additional body part meshes (heads, horns, claws,
/// armor) attached to a creature.
///
/// Modders can add, remove, and reorder `.nif` filenames in the list. Each row
/// displays whether the mesh file is found in the game's VFS archives or loose files.
pub fn model_list_widget(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    form_id: u32,
    _rec_sig: &[u8; 4],
    _edit: &mut crate::panels::FieldEdit,
) {
    let mut nifs: Vec<String> = session
        .resolved_record(form_id)
        .and_then(|r| r.subrecords().ok())
        .and_then(|subs| subs.into_iter().find(|s| &s.sig == b"NIFZ"))
        .map(|s| {
            String::from_utf8_lossy(&s.data)
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let folder = session
        .model_path_for(form_id)
        .and_then(|p| p.rsplit_once('\\').map(|(d, _)| d.to_string()))
        .unwrap_or_else(|| "meshes".to_string());

    let mut touched = false;
    let mut remove_idx = None;
    let mut swap_idx = None;

    if nifs.is_empty() {
        ui.label("No model meshes in list.");
    } else {
        let len = nifs.len();
        egui::Grid::new("model_list_grid")
            .striped(true)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.strong("#");
                ui.strong("Model Mesh (.nif)");
                ui.strong("Status");
                ui.strong("Actions");
                ui.end_row();

                for (i, name) in nifs.iter_mut().enumerate() {
                    ui.monospace(format!("[{i}]"));

                    let mut current_name = name.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut current_name).desired_width(240.0))
                        .changed()
                    {
                        *name = current_name.clone();
                        touched = true;
                    }

                    let full_path = if current_name.contains('\\') || current_name.contains('/') {
                        nvre_vfs::normalize(&current_name)
                    } else {
                        format!("{folder}\\{current_name}")
                    };

                    if session.vfs_exists(&full_path) {
                        ui.colored_label(egui::Color32::from_rgb(100, 200, 120), "✔ Found");
                    } else {
                        ui.colored_label(egui::Color32::from_rgb(220, 100, 100), "⚠ Missing");
                    }

                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(i > 0, egui::Button::new("Up").small())
                            .clicked()
                        {
                            swap_idx = Some((i, i - 1));
                        }
                        if ui
                            .add_enabled(i + 1 < len, egui::Button::new("Down").small())
                            .clicked()
                        {
                            swap_idx = Some((i, i + 1));
                        }
                        if ui.small_button("✖").on_hover_text("Remove this mesh").clicked() {
                            remove_idx = Some(i);
                        }
                    });
                    ui.end_row();
                }
            });
    }

    ui.add_space(4.0);
    let new_nif_id = ui.make_persistent_id("new_nif_input");
    let mut new_nif = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(new_nif_id))
        .unwrap_or_default();

    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut new_nif)
                .hint_text("NewMesh.nif")
                .desired_width(200.0),
        );
        if ui.button("+ Add Model").clicked() {
            let trimmed = new_nif.trim();
            if !trimmed.is_empty() {
                nifs.push(trimmed.to_string());
                touched = true;
                new_nif.clear();
            }
        }
        if ui
            .button("Browse...")
            .on_hover_text("Browse for a mesh file to add to the model list")
            .clicked()
        {
            let start_dir = session.data_dir.join(&folder);
            let mut dialog = rfd::FileDialog::new()
                .set_title("Select Mesh to Add")
                .add_filter("NIF Mesh (*.nif)", &["nif", "NIF"]);
            if start_dir.is_dir() {
                dialog = dialog.set_directory(&start_dir);
            } else if session.data_dir.join("meshes").is_dir() {
                dialog = dialog.set_directory(session.data_dir.join("meshes"));
            }
            if let Some(chosen) = dialog.pick_file() {
                let filename = chosen
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                if !filename.is_empty() {
                    nifs.push(filename);
                    touched = true;
                }
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(new_nif_id, new_nif));

    if let Some((a, b)) = swap_idx {
        nifs.swap(a, b);
        touched = true;
    }
    if let Some(idx) = remove_idx {
        nifs.remove(idx);
        touched = true;
    }
    if touched {
        if nifs.is_empty() {
            session.doc.remove_sub(form_id, b"NIFZ");
        } else {
            let mut bytes = Vec::new();
            for n in &nifs {
                bytes.extend_from_slice(n.as_bytes());
                bytes.push(0);
            }
            bytes.push(0);
            session.doc.set_sub(form_id, b"NIFZ", bytes);
        }
    }
}

/// The Object Bounds (`OBND`) editor: 3D bounding box for collision and placement.
///
/// Provides manual DragValue numeric editing for `min` and `max` (X, Y, Z), as well
/// as a "Calculate Bounds from Model" button that automatically parses the actor's
/// NIF geometry from the VFS to compute exact bounds.
pub fn object_bounds_widget(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    form_id: u32,
    _rec_sig: &[u8; 4],
    _edit: &mut crate::panels::FieldEdit,
) {
    let obnd = session
        .resolved_record(form_id)
        .and_then(|r| r.subrecords().ok())
        .and_then(|subs| subs.into_iter().find(|s| &s.sig == b"OBND"))
        .map(|s| s.data)
        .unwrap_or_else(|| vec![0; 12]);

    let i16_at = |o: usize| -> i16 {
        if obnd.len() >= o + 2 {
            i16::from_le_bytes([obnd[o], obnd[o + 1]])
        } else {
            0
        }
    };

    let mut min = [i16_at(0), i16_at(2), i16_at(4)];
    let mut max = [i16_at(6), i16_at(8), i16_at(10)];
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.strong("Min:");
        ui.label("min.x");
        if ui.add(egui::DragValue::new(&mut min[0])).changed() {
            changed = true;
        }
        ui.label("min.y");
        if ui.add(egui::DragValue::new(&mut min[1])).changed() {
            changed = true;
        }
        ui.label("min.z");
        if ui.add(egui::DragValue::new(&mut min[2])).changed() {
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        ui.strong("Max:");
        ui.label("max.x");
        if ui.add(egui::DragValue::new(&mut max[0])).changed() {
            changed = true;
        }
        ui.label("max.y");
        if ui.add(egui::DragValue::new(&mut max[1])).changed() {
            changed = true;
        }
        ui.label("max.z");
        if ui.add(egui::DragValue::new(&mut max[2])).changed() {
            changed = true;
        }

        ui.add_space(16.0);
        if ui
            .button("Calculate Bounds from Model")
            .on_hover_text("Compute 3D bounding box from NIF geometry in VFS")
            .clicked()
        {
            if let Some((calc_min, calc_max)) = session.calculate_bounds_for(form_id) {
                min = calc_min;
                max = calc_max;
                changed = true;
            }
        }
    });

    if changed {
        let mut bytes = Vec::with_capacity(12);
        for v in min.iter().chain(max.iter()) {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        session.doc.set_sub(form_id, b"OBND", bytes);
    }
}


/// The inventory table: one row per `CNTO`, with the `COED` that follows it
/// folded into the same row.
///
/// GECK shows an actor's inventory as a single table — item, count, and the
/// per-item extras (condition, ownership) on the same row. `CNTO` and `COED` are
/// *separate subrecords*, and the `COED` applies to the `CNTO` immediately
/// before it, so the two lists have to be zipped by position rather than by
/// signature. Two `set_sub_list` calls would do that wrong: the encoder appends
/// each signature's list at the first occurrence of that signature, which would
/// hoist every `COED` above every `CNTO` and destroy the pairing.
///
/// So the whole `CNTO`/`COED` run is rewritten as one interleaved sequence via
/// [`nvre_edit::document::Document::set_sub_sequence`], which preserves the
/// original interleaving and leaves every other subrecord in place.
///
/// The table is a real [`egui_extras::TableBuilder`] — the same widget the Cell
/// Window's and Object Window's lists use — so the rows virtualize: a creature
/// with 8,000 items draws only the ones on screen.
pub fn inventory_widget(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    form_id: u32,
    rec_sig: &[u8; 4],
    edit: &mut FieldEdit,
) {
    let Some(rec) = session.resolved_record(form_id) else {
        ui.colored_label(egui::Color32::RED, format!("{form_id:08X} not found"));
        return;
    };
    let Ok(subs) = rec.subrecords() else {
        return;
    };
    // The CNTO/COED run, in file order, with every other subrecord dropped: this
    // is exactly what `set_sub_sequence` rewrites, so an add/remove/move writes
    // the run back in the order the table shows it.
    let mut run: Vec<([u8; 4], Vec<u8>)> = subs
        .iter()
        .filter(|s| &s.sig == b"CNTO" || &s.sig == b"COED")
        .map(|s| (s.sig, s.data.clone()))
        .collect();

    // One row per CNTO, carrying the COED that follows it (if any).
    struct InvRow {
        cnto_bytes: Vec<u8>,
        coed_bytes: Option<Vec<u8>>,
        item: serde_json::Value,
        count: serde_json::Value,
        condition: Option<f32>,
        owner: Option<u32>,
        rank_or_global: Option<u32>,
    }
    let mut rows: Vec<InvRow> = Vec::new();
    {
        let mut pending: Option<InvRow> = None;
        for (sig, data) in &run {
            let sub = nvre_esm::Subrecord {
                sig: *sig,
                data: data.clone(),
                extended: false,
            };
            let decoded = nvre_edit::encode::decoded_in_block_localized(
                &session.schema,
                rec_sig,
                &sub,
                None,
                session.record_is_localized(form_id),
            )
            .unwrap_or(serde_json::Value::Null);
            if sig == b"CNTO" {
                if let Some(r) = pending.take() {
                    rows.push(r);
                }
                pending = Some(InvRow {
                    cnto_bytes: data.clone(),
                    coed_bytes: None,
                    item: decoded.get("item").cloned().unwrap_or(serde_json::json!(0)),
                    count: decoded.get("count").cloned().unwrap_or(serde_json::json!(1)),
                    condition: None,
                    owner: None,
                    rank_or_global: None,
                });
            } else if let Some(r) = pending.as_mut() {
                r.coed_bytes = Some(data.clone());
                r.condition = decoded
                    .get("item_condition")
                    .and_then(|v| v.as_f64())
                    .map(|f| f as f32);
                r.owner =
                    formid_from_json(decoded.get("owner").unwrap_or(&serde_json::Value::Null));
                r.rank_or_global = decoded
                    .get("global_or_rank_raw")
                    .and_then(|v| v.as_u64())
                    .map(|u| u as u32);
            }
        }
        if let Some(r) = pending.take() {
            rows.push(r);
        }
    }

    // What the table chose. `want` is the whole run to write back; the others
    // are rebuilt into a run after the table has been drawn.
    let mut want: Option<Vec<([u8; 4], Vec<u8>)>> = None;
    let mut remove_at: Option<usize> = None;
    let mut move_up: Option<usize> = None;
    let mut move_down: Option<usize> = None;
    // What the row's extras became, per row: the owner, the rank-or-global and
    // the condition in percent. The table's cells write into these as they are
    // drawn, and the COED is re-encoded from them once the row is complete —
    // reading the values back out of `rows` instead would write the *pre-edit*
    // bytes and silently discard the change.
    let mut pending_owner: Vec<Option<u32>> = rows.iter().map(|r| r.owner.filter(|o| *o != 0)).collect();
    let mut pending_rank: Vec<u32> = rows.iter().map(|r| r.rank_or_global.unwrap_or(0)).collect();
    let mut pending_cond: Vec<i64> = rows
        .iter()
        .map(|r| (r.condition.unwrap_or(1.0) * 100.0).round() as i64)
        .collect();
    let mut extras_changed: Vec<bool> = vec![false; rows.len()];

    // The rank titles of every faction, in the order `RNAM` gives them: a
    // faction's ranks are named by its own records, so the dropdown can offer
    // "Squire" rather than a bare 0..7 and a modder can see which rank is
    // which. The titles repeat once per rank, so the list is walked in order.
    let faction_ranks: std::collections::HashMap<u32, Vec<(i32, String)>> = {
        let mut out: std::collections::HashMap<u32, Vec<(i32, String)>> =
            std::collections::HashMap::new();
        for (fid, _) in session.records_of(*b"FACT").iter() {
            let Some(rec) = session.resolved_record(*fid) else {
                continue;
            };
            let Ok(subs) = rec.subrecords() else {
                continue;
            };
            let mut ranks: Vec<(i32, String)> = Vec::new();
            let mut last_rank: Option<i32> = None;
            for s in subs.iter() {
                match &s.sig {
                    b"RNAM" if s.data.len() >= 4 => {
                        last_rank = Some(i32::from_le_bytes(s.data[..4].try_into().unwrap()));
                    }
                    b"MNAM" if last_rank.is_some() => {
                        ranks.push((
                            last_rank.take().unwrap(),
                            String::from_utf8_lossy(&s.data).trim_end_matches('\0').to_string(),
                        ));
                    }
                    _ => {}
                }
            }
            if !ranks.is_empty() {
                out.insert(*fid, ranks);
            }
        }
        out
    };

    // Not resizable, and the columns fill the panel's width rather than
    // stopping at their initial widths: this table is nested inside the record
    // editor's scroll area, where a resizable table's handles fight the scroll
    // and its initial widths leave a dead strip to the right of the last
    // column. `Item` is the remainder column, so the space a long item name
    // needs comes out of the slack instead of being clipped.
    egui_extras::TableBuilder::new(ui)
        .striped(true)
        .id_salt(("inventory_table", rec_sig))
        // Actions, Item, Count, Condition, Owner, Rank / Global.
        .column(egui_extras::Column::initial(88.0).at_least(88.0).clip(true))
        .column(egui_extras::Column::remainder().at_least(120.0).clip(true))
        .column(egui_extras::Column::initial(60.0).at_least(56.0))
        .column(egui_extras::Column::initial(72.0).at_least(60.0))
        .column(egui_extras::Column::initial(180.0).at_least(120.0).clip(true))
        .column(egui_extras::Column::initial(120.0).at_least(90.0).clip(true))
        .header(18.0, |mut header| {
            header.col(|ui| {
                ui.strong("");
            });
            header.col(|ui| {
                ui.strong("Item");
            });
            header.col(|ui| {
                ui.strong("Count");
            });
            header.col(|ui| {
                ui.strong("Condition");
            });
            header.col(|ui| {
                // The owner may be an actor or a faction, and which one it is
                // decides what the last column means: a `FACT` owner makes it a
                // Required Rank, an `NPC_` owner makes it a Global Variable.
                ui.strong("Owner (NPC / Faction)");
            });
            header.col(|ui| {
                ui.strong("Rank / Global");
            });
        })
        .body(|body| {
            body.rows(20.0, rows.len(), |mut row_ui| {
                let i = row_ui.index();
                let Some(row) = rows.get(i) else {
                    return;
                };
                row_ui.col(|ui| {
                    ui.push_id(i, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .small_button("✖")
                                .on_hover_text("remove this item")
                                .clicked()
                            {
                                remove_at = Some(i);
                            }
                            let up = ui.add_enabled(i > 0, egui::Button::new("Up").small());
                            if up.on_hover_text("move up").clicked() {
                                move_up = Some(i);
                            }
                            let down = ui
                                .add_enabled(i + 1 < rows.len(), egui::Button::new("Down").small());
                            if down.on_hover_text("move down").clicked() {
                                move_down = Some(i);
                            }
                        });
                    });
                });

                // Item — the filtered picker, over the item types fopdoc names.
                //
                // `next` and `touched` are declared here and used by the Count
                // column below, so both columns have to be drawn before the row
                // writes the CNTO back — which happens after the last cell.
                let mut next =
                    serde_json::json!({"item": formid_from_json(&row.item).unwrap_or(0), "count": row.count.clone()});
                let mut touched = false;
                row_ui.col(|ui| {
                    let targets = crate::object_editor::formid_targets_in(
                        &session.schema,
                        rec_sig,
                        b"CNTO",
                        None,
                        "item",
                    );
                    let mut opt = formid_from_json(&row.item);
                    let before = opt;
                    let items = session.records_of_any(&targets);
                    crate::object_editor::record_dropdown(
                        ui,
                        &format!("inv_{i}_item"),
                        &items,
                        &mut opt,
                    );
                    touched = opt != before;
                    if touched {
                        set_inline_field(&mut next, "item", serde_json::json!(opt.unwrap_or(0)));
                    }
                });

                // Count. Its own column: the item picker fills the Item column's
                // whole height, so a Count left inside it would sit under the
                // Item header and shift every column after it left by one.
                row_ui.col(|ui| {
                    let mut cnt = row.count.as_i64().unwrap_or(1);
                    if ui
                        .add_sized(
                            [56.0, 18.0],
                            egui::DragValue::new(&mut cnt).range(0..=1_000_000),
                        )
                        .changed()
                    {
                        set_inline_field(&mut next, "count", serde_json::json!(cnt));
                        touched = true;
                    }
                });

                // Item Condition — percent, the way GECK shows it. A row with no
                // COED has no stored condition and reads 100%.
                row_ui.col(|ui| {
                    let mut cond_pct = pending_cond[i];
                    if ui
                        .add_sized(
                            [64.0, 18.0],
                            egui::DragValue::new(&mut cond_pct).range(0..=100).suffix("%"),
                        )
                        .changed()
                    {
                        pending_cond[i] = cond_pct;
                        extras_changed[i] = true;
                    }
                });

                // Owner — a NPC_ or FACT FormID.
                row_ui.col(|ui| {
                    let mut owner_opt = pending_owner[i];
                    let owner_before = owner_opt;
                    let owner_items =
                        session.records_of_any(&["NPC_".to_string(), "FACT".to_string()]);
                    crate::object_editor::record_dropdown_with_none(
                        ui,
                        &format!("inv_{i}_owner"),
                        &owner_items,
                        &mut owner_opt,
                        "Nobody",
                    );
                    if owner_opt != owner_before {
                        pending_owner[i] = owner_opt;
                        extras_changed[i] = true;
                    }
                });

                // Global Variable / Required Rank — a GLOB FormID when the owner
                // is an actor, an integer rank when the owner is a faction.
                //
                // With no owner set the field is inert: a rank with no faction
                // to be a rank *of*, and a global with no owner to gate it, are
                // both meaningless, and the only way to make either meaningful
                // is to set the owner first. The cell still shows what the bytes
                // hold, so a hand-written value is visible — it just cannot be
                // changed until the owner says which of the two it is.
                row_ui.col(|ui| {
                    let Some(owner) = pending_owner[i] else {
                        let shown = if pending_rank[i] == 0 {
                            "—".to_string()
                        } else {
                            format!("{:08X}", pending_rank[i])
                        };
                        ui.add_enabled(
                            false,
                            egui::TextEdit::singleline(&mut shown.clone())
                                .id_salt(format!("inv_{i}_rank_locked"))
                                .desired_width(100.0),
                        );
                        return;
                    };
                    let is_faction = session.record_sig(owner) == Some(*b"FACT");
                    if is_faction {
                        // Faction mode: a rank out of that faction's own ranks,
                        // so the dropdown names them ("Squire", "Knight") and
                        // cannot offer a rank the faction does not have.
                        let ranks = faction_ranks.get(&owner);
                        if let Some(ranks) = ranks {
                            let mut picked = pending_rank[i] as i32;
                            let cur = ranks
                                .iter()
                                .find(|(v, _)| *v == picked)
                                .map(|(_, n)| format!("{picked} - {n}"))
                                .unwrap_or_else(|| picked.to_string());
                            egui::ComboBox::from_id_salt(format!("inv_{i}_rank"))
                                .selected_text(cur)
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    for (v, n) in ranks {
                                        ui.selectable_value(&mut picked, *v, format!("{v} - {n}"));
                                    }
                                });
                            if picked.max(0) as u32 != pending_rank[i] {
                                pending_rank[i] = picked.max(0) as u32;
                                extras_changed[i] = true;
                            }
                        } else {
                            // A faction the schema gives no named ranks for: there is
                            // nothing to pick, so the cell reads None and is inert.
                            // (The rank itself still lives in the bytes and still
                            // round-trips — this only refuses to offer a choice
                            // between values the faction does not define.)
                            let mut none_label = "None".to_string();
                            ui.add_enabled(
                                false,
                                egui::TextEdit::singleline(&mut none_label)
                                    .id_salt(format!("inv_{i}_rank_none"))
                                    .desired_width(100.0),
                            );
                        }
                    } else {
                        let mut glob_opt = (pending_rank[i] != 0).then_some(pending_rank[i]);
                        let glob_before = glob_opt;
                        let glob_items = session.records_of_any(&["GLOB".to_string()]);
                        crate::object_editor::record_dropdown_with_none(
                            ui,
                            &format!("inv_{i}_glob"),
                            &glob_items,
                            &mut glob_opt,
                            "NONE",
                        );
                        if glob_opt != glob_before {
                            pending_rank[i] = glob_opt.unwrap_or(0);
                            extras_changed[i] = true;
                        }
                    }
                });

                // Write the CNTO back if either of its own fields changed. The
                // Nth CNTO in the run is this row's, so the position is found by
                // counting rather than by matching bytes (two identical items
                // exist).
                if touched {
                    if let Ok(bytes) = nvre_edit::encode::encode_sub_len_in_block(
                        &session.schema,
                        rec_sig,
                        b"CNTO",
                        &next,
                        Some(row.cnto_bytes.len()),
                        None,
                    ) {
                        let mut list: Vec<([u8; 4], Vec<u8>)> = run.iter().cloned().collect();
                        let mut seen = 0usize;
                        for (pos, (s, _)) in list.iter_mut().enumerate() {
                            if s == b"CNTO" {
                                if seen == i {
                                    list[pos].1 = bytes;
                                    want = Some(list);
                                    break;
                                }
                                seen += 1;
                            }
                        }
                    }
                }
            });
        });

    if rows.is_empty() {
        ui.add_space(4.0);
        ui.weak("No items");
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("+ Add Item").clicked() {
            let zero = match session.schema.lookup(rec_sig, b"CNTO") {
                Some(def) => zero_value(&session.schema.resolve_def(&def.ty)),
                None => serde_json::json!({"item": 0, "count": 1}),
            };
            if let Ok(bytes) = nvre_edit::encode::encode_sub_len_in_block(
                &session.schema,
                rec_sig,
                b"CNTO",
                &zero,
                None,
                None,
            ) {
                let mut list = run.clone();
                list.push((*b"CNTO", bytes));
                want = Some(list);
            }
        }
    });

    // A structural edit rebuilds the run from the rows, so a remove or move
    // writes the pairing back the way the table showed it.
    let rebuilt: Option<Vec<([u8; 4], Vec<u8>)>> = if let Some(i) = remove_at {
        let mut out: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for (ri, row) in rows.iter().enumerate() {
            if ri == i {
                continue;
            }
            out.push((*b"CNTO", row.cnto_bytes.clone()));
            if let Some(c) = &row.coed_bytes {
                out.push((*b"COED", c.clone()));
            }
        }
        Some(out)
    } else if move_up.is_some() || move_down.is_some() {
        let mut order: Vec<usize> = (0..rows.len()).collect();
        if let Some(i) = move_up {
            if i > 0 {
                order.swap(i, i - 1);
            }
        }
        if let Some(i) = move_down {
            if i + 1 < rows.len() {
                order.swap(i, i + 1);
            }
        }
        let mut out: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        for ri in order {
            let row = &rows[ri];
            out.push((*b"CNTO", row.cnto_bytes.clone()));
            if let Some(c) = &row.coed_bytes {
                out.push((*b"COED", c.clone()));
            }
        }
        Some(out)
    } else {
        None
    };
    if let Some(list) = rebuilt {
        want = Some(list);
    }

// The extras that changed: re-encode each of those rows' COED in place,
    // creating the subrecord when the row had none.
    //
    // One write per changed row, from `pending_*` — the values the table's
    // cells actually ended on — rather than from `rows`, which still holds the
    // bytes as they were read and would discard the edit.
    for i in 0..rows.len() {
        if !extras_changed.get(i).copied().unwrap_or(false) {
            continue;
        }
        let Some(row) = rows.get(i) else {
            continue;
        };
        let cond_pct = pending_cond[i];
        let owner_opt = pending_owner[i];
        let rank = pending_rank[i];
        let coed = serde_json::json!({
            "owner": owner_opt.unwrap_or(0),
            "global_or_rank_raw": rank,
            "item_condition": cond_pct as f64 / 100.0,
        });
        // The row had no COED and every extra is now the default: there is
        // nothing to store, and a zero COED is a different record from none.
        let all_default = cond_pct == 100 && owner_opt.is_none() && rank == 0;
        if row.coed_bytes.is_none() && all_default {
            continue;
        }
        let Ok(bytes) = nvre_edit::encode::encode_sub_len_in_block(
            &session.schema,
            rec_sig,
            b"COED",
            &coed,
            row.coed_bytes.as_ref().map(|b| b.len()),
            None,
        ) else {
            continue;
        };
        let mut list = run.clone();
        let cnto_positions: Vec<usize> = list
            .iter()
            .enumerate()
            .filter(|(_, (s, _))| s == b"CNTO")
            .map(|(p, _)| p)
            .collect();
        let Some(&cpos) = cnto_positions.get(i) else {
            return;
        };
        // Replace the COED right after this CNTO, or insert one.
        match list.get(cpos + 1) {
            Some((s, _)) if s == b"COED" => list[cpos + 1].1 = bytes,
            _ => list.insert(cpos + 1, (*b"COED", bytes)),
        }
        want = Some(list.clone());
        run = list;
    }

    if let Some(list) = want {
        session
            .doc
            .set_sub_sequence(form_id, vec![*b"CNTO", *b"COED"], list);
        session.stale = true;
        edit.error = None;
    }
}

/// One value's widget, for a row inside a list.
///
/// The same widgets the flat grid offers — a drag for a number, a checkbox for a
/// bool, a picker for a FormID, a dropdown for an enum — for a value that lives
/// inside a repeated subrecord instead of a subrecord of its own. Returns whether
/// the value changed, and writes it into `next` so the caller can re-encode the row.
///
/// A row that could only show text would be a regression for the pages `repeat`
/// exists to replace: a leveled list's entry is a FormID and two numbers, and a
/// form list is nothing but FormIDs.
fn set_inline_field(next: &mut serde_json::Value, path: &str, val: serde_json::Value) {
    if path.is_empty() {
        *next = val;
    } else if let serde_json::Value::Object(map) = next {
        map.insert(path.to_string(), val);
    }
}

#[allow(clippy::too_many_arguments)]
fn inline_value(
    ui: &mut egui::Ui,
    session: &crate::session::Session,
    rec_sig: &[u8; 4],
    sub_sig: &[u8; 4],
    block: Option<&str>,
    row_idx: usize,
    path: &str,
    value: &serde_json::Value,
    next: &mut serde_json::Value,
) -> bool {
    // A FormID gets the filtered picker, the same one the flat grid uses.
    if nvre_edit::encode::is_formid_field(&session.schema, rec_sig, sub_sig, block, path) {
        let targets =
            crate::object_editor::formid_targets_in(&session.schema, rec_sig, sub_sig, block, path);
        let mut opt = formid_from_json(value);
        let before = opt;
        let items = session.records_of_any(&targets);
        let id_salt = format!("{}_{}_{path}", nvre_util::tag_str(sub_sig), row_idx);
        crate::object_editor::record_dropdown(ui, &id_salt, &items, &mut opt);
        if opt != before {
            let val = match opt {
                Some(fid) => serde_json::json!(fid),
                None => serde_json::json!(0),
            };
            set_inline_field(next, path, val);
            return true;
        }
        return false;
    }
    // An enum gets its dropdown, for the same reason the grid does.
    let variants =
        nvre_edit::encode::enum_values_at(&session.schema, rec_sig, sub_sig, block, path);
    if !variants.is_empty() {
        if let Some(raw) = value.get("value").and_then(|v| v.as_i64()) {
            let is_bool_enum = variants.len() == 2
                && variants.iter().any(|(v, n)| *v == 0 && n == "False")
                && variants.iter().any(|(v, n)| *v == 1 && n == "True");
            if is_bool_enum {
                let mut checked = raw != 0;
                if ui.checkbox(&mut checked, "").changed() {
                    let picked = if checked { 1 } else { 0 };
                    let mut o = serde_json::Map::new();
                    o.insert("value".into(), serde_json::json!(picked));
                    o.insert(
                        "name".into(),
                        serde_json::json!(if checked { "True" } else { "False" }),
                    );
                    set_inline_field(next, path, serde_json::Value::Object(o));
                    return true;
                }
                return false;
            }
            let mut picked = raw;
            let cur = variants
                .iter()
                .find(|(v, _)| *v == raw)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| raw.to_string());
            let id_salt = format!("inline_{}_{}_{path}", nvre_util::tag_str(sub_sig), row_idx);
            egui::ComboBox::from_id_salt(id_salt)
                .selected_text(cur)
                .show_ui(ui, |ui| {
                    for (v, n) in &variants {
                        ui.selectable_value(&mut picked, *v, n);
                    }
                });
            if picked != raw {
                let mut o = serde_json::Map::new();
                o.insert("value".into(), serde_json::json!(picked));
                if let Some((_, n)) = variants.iter().find(|(v, _)| *v == picked) {
                    o.insert("name".into(), serde_json::json!(n));
                }
                set_inline_field(next, path, serde_json::Value::Object(o));
                return true;
            }
        }
        return false;
    }
    match value {
        serde_json::Value::Number(n) => {
            let is_int = n.is_i64() || n.is_u64();
            let mut v = n.as_f64().unwrap_or(0.0);
            let drag = egui::DragValue::new(&mut v).speed(if is_int { 1.0 } else { 0.1 });
            if ui.add_sized([70.0, 18.0], drag).changed() {
                let val = if is_int {
                    serde_json::json!(v.round() as i64)
                } else {
                    serde_json::json!(v)
                };
                set_inline_field(next, path, val);
                return true;
            }
            false
        }
        serde_json::Value::Bool(b) => {
            let mut v = *b;
            if ui.checkbox(&mut v, "").changed() {
                *next = serde_json::json!(v);
                return true;
            }
            false
        }
        serde_json::Value::String(s) => {
            let mut text = s.clone();
            if ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(140.0))
                .changed()
            {
                set_inline_field(next, path, serde_json::json!(text));
                return true;
            }
            false
        }
        other => {
            ui.monospace(other.to_string());
            false
        }
    }
}

/// A type's zero value, for a row or element that is being created.
fn zero_value(t: &nvre_esm::TypeDef) -> serde_json::Value {
    use nvre_esm::TypeDef;
    match t {
        TypeDef::Prim(nvre_esm::schema::Prim::F32) => serde_json::json!(0.0),
        TypeDef::Prim(_) | TypeDef::FormIdRef { .. } => serde_json::json!(0),
        TypeDef::Struct(fields) => {
            let mut o = serde_json::Map::new();
            for f in fields {
                if !f.optional {
                    o.insert(f.name.clone(), zero_value(&f.ty));
                }
            }
            serde_json::Value::Object(o)
        }
        TypeDef::Array { elem, count, .. } => {
            let n = count.unwrap_or(0);
            let elem_zero = zero_value(elem);
            serde_json::Value::Array(vec![elem_zero; n])
        }
        TypeDef::Enum { values, .. } => serde_json::json!({
            "value": values.first().map(|(v, _)| *v).unwrap_or(0),
        }),
        _ => serde_json::json!(0),
    }
}

/// The bits a flags field's schema names, as `(bit, name)` in bit order.
///
/// The decoder emits a flags field as `{"value": n, "set": [names], ...}` and
/// drops the bit numbers, so the only place they exist is the schema's
/// `{"flags": prim, "bits": {...}}` table. Reading them back from there is what
/// makes a checkbox write the right bit: guessing a bit from a name's position
/// would silently corrupt every flags field whose bits are not declared in
/// ascending order from zero.
///
/// `path` is the field's dotted path inside the subrecord, and it has to be
/// walked: a subrecord is usually a **struct**, and the flags word is one of its
/// fields. `ACBS` is a struct whose `flags` and `template_flags` fields are flags
/// words, so resolving the subrecord's own type answers `Struct` and finds
/// nothing — which is how a creature's flag checkboxes came to render as
/// `(object)` on every tab. The path is empty only for a subrecord that *is* a
/// flags word outright, which is what a top-level one is.
///
/// The block matters: a flags word inside a block (`PACK`'s `DATA`, `FACT`'s)
/// has the block's layout, not the record's top-level one.
fn flag_bits(
    schema: &nvre_esm::Schema,
    rec_sig: &[u8; 4],
    sub_sig: &str,
    block: Option<&str>,
    path: &str,
) -> Vec<(u32, String)> {
    let Some(def) = schema.lookup_in_block(rec_sig, &sig4(sub_sig), block) else {
        return Vec::new();
    };
    /// The type at a dotted path inside a resolved struct.
    fn at(resolved: &nvre_esm::TypeDef, parts: &[&str]) -> Option<nvre_esm::TypeDef> {
        let Some((part, rest)) = parts.split_first() else {
            return Some(resolved.clone());
        };
        match resolved {
            nvre_esm::TypeDef::Struct(fields) => {
                // The longest prefix that names a field, so a field whose own
                // name holds a dot is reachable rather than split.
                let mut best: Option<&nvre_esm::typedef::FieldDef> = None;
                for f in fields {
                    if part.len() >= f.name.len()
                        && part.starts_with(&f.name)
                        && best.is_none_or(|b| f.name.len() > b.name.len())
                    {
                        best = Some(f);
                    }
                }
                let f = best?;
                let tail = &part[f.name.len()..];
                let next: Vec<&str> = if tail.is_empty() {
                    rest.to_vec()
                } else {
                    let mut v = vec![tail.trim_start_matches('.')];
                    v.extend_from_slice(rest);
                    v
                };
                at(&f.ty, &next)
            }
            nvre_esm::TypeDef::Union(variants) => {
                for (_, v) in variants {
                    if let Some(found) = at(v, parts) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }
    let parts: Vec<&str> = if path.is_empty() {
        Vec::new()
    } else {
        path.split('.').collect()
    };
    match at(&schema.resolve_def(&def.ty), &parts) {
        Some(nvre_esm::typedef::TypeDef::Flags { bits, .. }) => bits,
        _ => Vec::new(),
    }
}

/// The property grid's own state: the last field touched, and why it failed.
///
/// A `Local` rather than a resource because it belongs to one panel's window:
/// the grid is the only thing that writes it, and a per-panel `Local` is what
/// keeps the panels' widget state apart. Public because a `Local<T>` parameter
/// makes `T` part of the system's signature, which the app registering the
/// system has to name (the same reason [`crate::viewport::ClickState`] is).
#[derive(Default)]
pub struct FieldEdit {
    last: Option<([u8; 4], String)>,
    error: Option<String>,
}

/// How a usage count is shown in the object window's Count and Users columns.
///
/// A zero is an em dash rather than a `0`: the columns answer "is this object
/// used at all" before they answer "how much", and a column of dashes reads as
/// unused at a glance where a column of zeros reads as a number to compare.
/// The dash is dimmed, so the number beside it is the one the eye lands on.
fn count_cell(n: usize) -> egui::RichText {
    if n == 0 {
        egui::RichText::new("—").weak()
    } else {
        egui::RichText::new(n.to_string())
    }
}

/// How wide the object palette's category column is, in points.
///
/// GECK's Object Window fixes its category pane at about this much, which is
/// enough for "Non-Player Character" and the object count without swallowing
/// the pane a modder actually picks from.
const CATEGORY_WIDTH: f32 = 180.0;

/// The gap between the palette's two columns, wide enough for a separator.
const SEPARATOR_GAP: f32 = 8.0;

/// The object palette: a category list down the left side, that category's
/// objects on the right.
///
/// GECK's Object Window is laid out this way, and it is what makes all game
/// objects navigable: a modder picks "Static" and sees statics rather than
/// scrolling past containers and lights to find them. The categories are grouped
/// under GECK's top-level sections ("Actors", "Items", "WorldObjects", ...).
/// That category's objects are a table — Editor ID, FormID, Name — the way
/// the game's own window lists them. Clicking a row selects it;
/// double-clicking opens it in the object editor, which the schema says
/// starts on which page.
pub fn place(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<3>>>,
    mut session: ResMut<crate::session::Session>,
    mut filters: ResMut<Filters>,
    mut chosen: Local<usize>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "place"), |ui| {
        ui.horizontal(|ui| {
            ui.label("filter");
            ui.add(
                egui::TextEdit::singleline(&mut filters.place)
                    .hint_text("name or Editor ID")
                    .desired_width((ui.available_width() - 260.0).max(60.0)),
            );
            // Which plugins the list draws from. A dropdown rather than a
            // single toggle because a load order is a stack of masters and a
            // mod, and which of them a modder wants to see is not one
            // question: hiding the masters they are not working with while
            // still seeing the game's base objects is the common case, and
            // neither "all plugins" nor "this plugin only" is it.
            //
            // The checkboxes are inside the popup rather than a list of
            // mutually-exclusive options, so every combination is reachable
            // and the list shows what is currently on. The two buttons at the
            // top are the ends of the range, so the common cases are one click
            // rather than a click per master.
            let flags = session.object_source_flags().to_vec();
            let shown = flags.iter().filter(|f| **f).count();
            let total = flags.len();
            // The one plugin still shown, when there is only one. Named rather
            // than assumed to be the plugin being edited: that one can be
            // hidden now, so "only" has to say which plugin it is.
            let only_one = if shown == 1 {
                session
                    .object_sources()
                    .into_iter()
                    .find(|(pi, _)| flags.get(*pi).copied().unwrap_or(false))
                    .map(|(_, n)| n.to_string())
            } else {
                None
            };
            let summary = if shown == total {
                "all plugins".to_string()
            } else if let Some(name) = &only_one {
                format!("{name} only")
            } else {
                format!("{shown} of {total} plugins")
            };
            // The names, out of the session: the popup below writes to the
            // session through its setters, which needs the borrow the list
            // itself was holding.
            let names: Vec<String> = session
                .object_sources()
                .into_iter()
                .map(|(_, n)| n.to_string())
                .collect();
            egui::ComboBox::from_id_salt("object_sources")
                .selected_text(summary)
                .width(170.0)
                .show_ui(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("all").clicked() {
                            session.set_object_sources_all(true);
                            ui.close();
                        }
                        if ui.button("this plugin").clicked() {
                            session.set_object_sources_all(false);
                            ui.close();
                        }
                    });
                    ui.separator();
                    for (pi, name) in names.into_iter().enumerate() {
                        // Every plugin is a plain checkbox, the plugin being
                        // edited included: a modder narrowing the list to a
                        // master has said they want the master's objects, and
                        // their own plugin's rows are the one thing they can
                        // already see everywhere else. Hiding it leaves the
                        // masters' objects and nothing else, which the window
                        // says rather than quietly refilling.
                        let mut on = flags.get(pi).copied().unwrap_or(true);
                        if ui.checkbox(&mut on, &name).clicked() {
                            if let Err(e) = session.set_object_source(pi, on) {
                                session.status = e;
                            }
                        }
                    }
                });
        });
        ui.separator();
        // Cached in the session and rebuilt only when the filter changes: the
        // query walks the load order, which costs hundreds of milliseconds, and
        // the panel draws every frame. Borrowed rather than cloned.
        let selected_fid = session.selected;
        // The flags the list is built from, read fresh here rather than held
        // from the dropdown above: that one drew its checkboxes from a copy
        // taken before it opened, and a plugin unchecked inside it has to
        // reach the list on the same frame.
        let sources = session.object_source_flags().to_vec();
        let source_count = sources.iter().filter(|f| **f).count();
        // The two usage columns, read before the list is borrowed. Both walks
        // cost seconds — the references table for the copies, every record's
        // fields for the users — and they are cached on the session, so this
        // is a hash lookup per row rather than a walk per frame. The first
        // frame that draws a column pays for it, which is why the columns are
        // built here rather than lazily inside the row closure.
        let usage = session.usage().clone();
        if session.object_groups(&filters.place, &sources).is_empty() {
            // Which plugins were asked for, so the empty state can say what
            // would fill it. "This plugin only" is the common case: a mod with
            // no records of its own yet is empty, and saying so is better than
            // "no objects match", which reads as though the game has none.
            let asked = session
                .object_sources()
                .into_iter()
                .filter(|(pi, _)| sources.get(*pi).copied().unwrap_or(false))
                .map(|(_, n)| n.to_string())
                .collect::<Vec<_>>();
            ui.label(match asked.as_slice() {
                [] => "every plugin is hidden — show one from the dropdown".to_string(),
                [name] => format!("{name} has no objects that match"),
                _ => "no objects match".to_string(),
            });
            return;
        }
        let groups = session.object_groups(&filters.place, &sources);
        // Clamp the selection into range. A filter that narrows to one category
        // selects it, so typing a name does not leave the right half of the
        // window empty behind a category the filter emptied out. A filter that
        // removes categories shifts the indices, so a selection past the new end
        // lands on the last category rather than indexing out of bounds.
        *chosen = (*chosen).min(groups.len().saturating_sub(1));
        // Every count the footer needs is read out before the action is
        // placed, which needs `&mut Session` while `groups` still borrows it.
        let total: usize = groups.iter().map(|g| g.objects.len()).sum();
        let empty = groups.iter().filter(|g| g.objects.is_empty()).count();
        let shown = groups.len() - empty;

        let cur_group = &groups[*chosen];
        let mut open_obj: Option<(u32, [u8; 4])> = None;
        let mut select_fid: Option<u32> = None;
        // The row the Duplicate prompt is opened on, kept across the frame:
        // the prompt is a modal drawn after this list, and it needs the row's
        // Editor ID to prefill the copy's name.
        let mut ask_duplicate: Option<(u32, [u8; 4], String)> = None;
        // The row Delete was chosen on. The menu runs inside the row's
        // closure, which cannot take the session while the list borrows it,
        // so the FormID is carried out and acted on below.
        let mut delete_fid: Option<u32> = None;

        // The two halves of GECK's Object Window: a category list down the LEFT
        // side, the chosen category's objects on the right.
        let list = egui::Rect::from_min_size(
            ui.next_widget_position(),
            egui::vec2(CATEGORY_WIDTH, ui.available_height()),
        );
        {
            let mut ui = egui::Ui::new(
                ui.ctx().clone(),
                "categories".into(),
                egui::UiBuilder::new().max_rect(list),
            );
            crate::panels::vertical_scroll("categories").show(&mut ui, |ui| {
                let mut last_group = "";
                for (i, g) in groups.iter().enumerate() {
                    if g.group != last_group {
                        last_group = &g.group;
                        let group_count: usize = groups
                            .iter()
                            .filter(|x| x.group == last_group)
                            .map(|x| x.objects.len())
                            .sum();
                        ui.add_space(6.0);
                        ui.strong(format!("{} ({})", last_group, group_count));
                    }
                    let selected = *chosen == i;
                    let label = if g.objects.is_empty() {
                        format!("  {}  —", g.category)
                    } else {
                        format!("  {}  {}", g.category, g.objects.len())
                    };
                    if ui.selectable_label(selected, label).clicked() {
                        *chosen = i;
                    }
                }
            });
        }
        // A separator down the gap between the columns, drawn as a manually
        // positioned line so it does not take part in the layout flow.
        ui.painter().vline(
            list.right() + SEPARATOR_GAP / 2.0,
            list.y_range(),
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
        let rest = egui::Rect::from_min_size(
            list.right_top() + egui::vec2(SEPARATOR_GAP, 0.0),
            egui::vec2(
                (ui.available_width() - CATEGORY_WIDTH - SEPARATOR_GAP).max(0.0),
                ui.available_height(),
            ),
        );
        let mut ui = egui::Ui::new(
            ui.ctx().clone(),
            "objects".into(),
            egui::UiBuilder::new().max_rect(rest),
        );
        if groups[*chosen].objects.is_empty() {
            ui.label("no objects in this category match the filter");
        }
        // The list is a table — Editor ID, FormID, Name — the way the
        // game's own object window lists them, so a modder can read and
        // scan each field on its own column instead of one merged label.
        // Each column has a width policy, so one long name clips instead
        // of stretching the table, and the Name column takes whatever
        // room is left.
        //
        // The row is the widget, not the labels in it — the egui demo's
        // own pattern: `Sense::click` on the table makes the whole row's
        // area clickable, `set_selected` paints the whole row, and the
        // row's response is the one the panel acts on. The table scrolls
        // its own body, so there is no ScrollArea around it.
        //
        // Columns start at a preset width and can be dragged — never
        // `auto()`, whose sizing pass renders the whole table invisibly
        // to measure content, which with unvirtualized rows meant every
        // object in the category twice. The Name column is the one
        // remainder: it absorbs the slack and re-flows as the others
        // are dragged.
        egui_extras::TableBuilder::new(&mut ui)
            .striped(true)
            .sense(egui::Sense::click())
            .resizable(true)
            .id_salt("objects_table")
            .column(
                egui_extras::Column::initial(240.0)
                    .at_least(120.0)
                    .clip(true),
            )
            .column(egui_extras::Column::initial(90.0).at_least(80.0))
            // Count and Users: two narrow, right-aligned number columns. They
            // are narrow because the numbers are small, and they are fixed
            // rather than remainders so the Name column keeps the slack. A
            // `0` is shown as an em dash, the way the reference tables do: a
            // modder scanning for an object the game never places reads a dash
            // faster than a zero.
            .column(egui_extras::Column::initial(64.0).at_least(48.0))
            .column(egui_extras::Column::initial(64.0).at_least(48.0))
            .column(egui_extras::Column::remainder().resizable(false).clip(true))
            .header(18.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Editor ID");
                });
                header.col(|ui| {
                    ui.strong("FormID");
                });
                header.col(|ui| {
                    ui.strong("Count")
                        .on_hover_text("copies of this object placed in the game's cells");
                });
                header.col(|ui| {
                    ui.strong("Users")
                        .on_hover_text("records and scripts that reference this object");
                });
                header.col(|ui| {
                    ui.strong("Name");
                });
            })
            .body(|body| {
                // `rows` virtualizes: only the rows in the viewport are
                // visited, so the 6,785 statics cost what any small
                // category costs. The index is the row's own.
                let objects = &groups[*chosen].objects;
                body.rows(18.0, objects.len(), |mut row_ui| {
                    let row = &objects[row_ui.index()];
                    // The whole row highlights when it is the selected
                    // one — background and text — not one label in it.
                    row_ui.set_selected(selected_fid == Some(row.fid));
                    row_ui.col(|ui| {
                        ui.label(&row.edid);
                    });
                    row_ui.col(|ui| {
                        ui.label(egui::RichText::new(format!("{:08X}", row.fid)).monospace());
                    });
                    row_ui.col(|ui| {
                        ui.label(count_cell(usage.copies(row.fid)));
                    });
                    row_ui.col(|ui| {
                        ui.label(count_cell(usage.users(row.fid)));
                    });
                    row_ui.col(|ui| {
                        ui.label(&row.name);
                    });
                    let resp = row_ui.response();
                    // Right-clicking a row opens its context menu. Edit is
                    // what double-clicking does; Duplicate asks for a name
                    // first, prefilled with this row's Editor ID plus COPY.
                    resp.context_menu(|ui| {
                        select_fid = Some(row.fid);
                        if ui.button("Edit").clicked() {
                            open_obj = Some((row.fid, cur_group.sig));
                            ui.close();
                        }
                        if ui.button("Duplicate").clicked() {
                            ask_duplicate = Some((row.fid, cur_group.sig, row.edid.clone()));
                            ui.close();
                        }
                        // Delete is the same action the References window's
                        // button makes, on the row the menu was opened on
                        // rather than on the selection: right-clicking a row
                        // already points at it. Nothing is written until the
                        // save, and the status line says which record was
                        // queued — the row leaves the list immediately, because
                        // a record the game will not have is not one the
                        // window should list.
                        if ui.button("Delete").clicked() {
                            delete_fid = Some(row.fid);
                            ui.close();
                        }
                    });
                    if resp.double_clicked() {
                        open_obj = Some((row.fid, cur_group.sig));
                    } else if resp.clicked() {
                        select_fid = Some(row.fid);
                    }
                });
            });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} objects in {} categories{}",
                total,
                shown,
                if empty == 0 {
                    String::new()
                } else {
                    format!(" ({empty} empty)")
                }
            ));
            // Which plugins those objects came from. The same count reads
            // differently against the plugin's own records than against every
            // plugin's, and a footer that does not say which is the one thing
            // a modder cannot tell from the rows themselves.
            if source_count < sources.len() {
                ui.label(format!("from {source_count} of {} plugins", sources.len()));
            }
            // The per-row Edit button became one Edit for the selected row,
            // now that the list is a table. It is enabled only for a row of
            // the category on show, so a reference selected in the viewport
            // cannot be opened as this category's record type.
            let editable = groups[*chosen]
                .objects
                .iter()
                .any(|r| Some(r.fid) == selected_fid);
            if ui
                .add_enabled(editable, egui::Button::new("Edit"))
                .clicked()
            {
                open_obj = Some((selected_fid.unwrap_or_default(), cur_group.sig));
            }
        });
        // The actions, after every piece of UI that can choose one.
        //
        // Delete before the others: it is the one that takes the row out of
        // the list, and a row that has just left the list should not also be
        // selected or opened.
        if let Some(fid) = delete_fid {
            session.delete_object(fid);
        } else if let Some((fid, sig)) = open_obj {
            session.open_object(fid, sig);
        } else if let Some(fid) = select_fid {
            session.select(fid);
            session.status = format!("selected {fid:08X}");
        }
        // The Duplicate prompt, opened on the row the menu was opened on. It
        // is a modal rather than another inline form: a name has to be typed
        // before anything is written, and the backdrop keeps the click that
        // opened it from reaching the table behind.
        if let Some((fid, sig, edid)) = ask_duplicate {
            filters.duplicate = Some(DuplicatePrompt {
                fid,
                sig,
                // The prefill is the row's own Editor ID with COPY on it, so
                // the common case is one keystroke. A row with no Editor ID
                // still gets a usable name: an empty one would be refused.
                edid: format!("{edid}COPY"),
            });
        }
        duplicate_prompt(ui.ctx(), &mut session, &mut filters);
    });
}

/// The prompt Duplicate opens: the copy's Editor ID, prefilled and editable.
///
/// `session` and `filters` are separate because the prompt writes to both —
/// the duplicate is queued through the session, and the prompt's own field
/// lives in the filters, the same place the new-cell and new-worldspace forms
/// keep theirs. Returning `filters.duplicate` to `None` is how the prompt
/// closes, and it happens on both paths: confirming writes the duplicate, and
/// cancelling leaves the session untouched.
fn duplicate_prompt(
    ctx: &egui::Context,
    session: &mut crate::session::Session,
    filters: &mut Filters,
) {
    let Some(prompt) = filters.duplicate.as_mut() else {
        return;
    };
    let fid = prompt.fid;
    let sig = prompt.sig;
    let mut done = false;
    egui::Modal::new(egui::Id::new("duplicate_prompt")).show(ctx, |ui| {
        ui.heading("Duplicate");
        ui.label(format!(
            "A copy of {fid:08X}, written with the Editor ID below. \
             Save to write it into the plugin."
        ));
        ui.horizontal(|ui| {
            ui.label("Editor ID");
            // The field has to be focused for the prefill to be replaced by
            // typing: without focus the first keystroke goes nowhere, and the
            // user watches a name they did not type.
            let field = ui.text_edit_singleline(&mut prompt.edid);
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                match session.duplicate_object(fid, sig, &prompt.edid) {
                    Ok(new) => {
                        session.status =
                            format!("duplicated {fid:08X} as {new:08X}; save to write it");
                        done = true;
                    }
                    Err(e) => session.status = e,
                }
            }
            field.request_focus();
        });
        ui.horizontal(|ui| {
            if ui.button("Duplicate").clicked() {
                match session.duplicate_object(fid, sig, &prompt.edid) {
                    Ok(new) => {
                        session.status =
                            format!("duplicated {fid:08X} as {new:08X}; save to write it");
                        done = true;
                    }
                    Err(e) => session.status = e,
                }
            }
            if ui.button("Cancel").clicked() {
                done = true;
            }
        });
    });
    if done {
        filters.duplicate = None;
    }
}

/// The status line: what the last action did, and how much is unsaved.
pub fn status(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<4>>>,
    mut session: ResMut<crate::session::Session>,
    mut gizmo: ResMut<crate::gizmo::GizmoState>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "status"), |ui| {
        ui.horizontal(|ui| {
            ui.label("Tool:");
            if ui
                .selectable_label(gizmo.mode == crate::gizmo::GizmoMode::Translate, "W: Move")
                .clicked()
            {
                gizmo.mode = crate::gizmo::GizmoMode::Translate;
            }
            if ui
                .selectable_label(gizmo.mode == crate::gizmo::GizmoMode::Rotate, "E: Rotate")
                .clicked()
            {
                gizmo.mode = crate::gizmo::GizmoMode::Rotate;
            }
            if ui
                .selectable_label(gizmo.mode == crate::gizmo::GizmoMode::Scale, "R: Scale")
                .clicked()
            {
                gizmo.mode = crate::gizmo::GizmoMode::Scale;
            }
            ui.separator();
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
/// The camera is the one thing in the editor whose state is worth watching while
/// it moves: a viewport that shows nothing is usually the camera looking somewhere
/// else, and the numbers say where. Editing them here moves the camera the same
/// way framing does, which makes a framing problem something you can dial in
/// rather than hunt for.
///
/// What is shown is read off the camera itself, not from a copy of what the
/// editor last asked for, so the readout describes the view rather than the
/// request.
///
/// The target is the point the camera is looking at, which a free camera has no
/// pivot to keep it aimed at: it is the eye pushed out along the heading by the
/// distance, so it follows the camera rather than the other way round. It can be
/// dialled in here — that is a request like any other — and the distance with it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn debug(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<crate::windows::PanelContext<5>>>,
    mut focus: ResMut<crate::viewport::ViewportFocus>,
    camera: Query<&Transform, With<crate::viewport::ViewportCamera>>,
    time: Res<Time>,
    spawned: Option<Res<crate::viewport::Spawned>>,
    meshes_query: Query<(), With<Mesh3d>>,
    lights_query: Query<(), Or<(With<PointLight>, With<SpotLight>, With<DirectionalLight>)>>,
    mut smoothed_fps: Local<f32>,
) {
    egui::CentralPanel::default().show(&mut root_ui(ctx.get_mut(), "debug"), |ui| {
        ui.heading("Performance");
        let dt = time.delta_secs();
        let instant_fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
        if *smoothed_fps == 0.0 {
            *smoothed_fps = instant_fps;
        } else {
            *smoothed_fps = *smoothed_fps * 0.9 + instant_fps * 0.1;
        }
        let fps = *smoothed_fps;
        let frame_ms = if fps > 0.0 { 1000.0 / fps } else { 0.0 };
        let fps_color = if fps >= 55.0 {
            egui::Color32::from_rgb(100, 220, 100)
        } else if fps >= 30.0 {
            egui::Color32::from_rgb(220, 200, 80)
        } else {
            egui::Color32::from_rgb(240, 80, 80)
        };
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{fps:5.1} FPS ({frame_ms:5.1} ms)"))
                    .color(fps_color)
                    .strong(),
            );
        });

        ui.add_space(2.0);
        egui::Grid::new("perf_stats").num_columns(2).show(ui, |ui| {
            let mesh_count = meshes_query.iter().count();
            ui.label("Active 3D Meshes:");
            ui.label(format!("{mesh_count}"));
            ui.end_row();

            let light_count = lights_query.iter().count();
            ui.label("Active Lights:");
            ui.label(format!("{light_count}"));
            ui.end_row();

            if let Some(ref sp) = spawned {
                ui.label("Spawned Refs:");
                ui.label(format!("{}", sp.refs.len()));
                ui.end_row();

                ui.label("Failed/Skipped Refs:");
                ui.label(format!("{}", sp.failed.len()));
                ui.end_row();

                ui.label("Models In Cache:");
                ui.label(format!("{}", sp.loaded_models.len()));
                ui.end_row();

                ui.label("Loading Status:");
                let status_str = match (sp.models_finished, sp.lights_finished) {
                    (true, true) => "Idle (All Loaded)",
                    (false, _) => "Loading Models...",
                    (_, false) => "Spawning Lights...",
                };
                ui.label(status_str);
                ui.end_row();
            }
        });

        ui.separator();
        ui.heading("Camera");

        // Where the camera is and what it is looking along, as it actually is.
        let (eye, forward) = match camera.single() {
            Ok(t) => (t.translation, *t.forward()),
            Err(_) => (Vec3::ZERO, Vec3::NEG_Z),
        };

        egui::Grid::new("orbit").num_columns(2).show(ui, |ui| {
            // The target is where the camera looks, in Bevy space. A cell sits
            // tens of thousands of units from the origin, so the values are shown
            // to a unit rather than the default two decimals.
            let mut target = [focus.target.x, focus.target.y, focus.target.z];
            ui.label("target");
            ui.horizontal(|ui| {
                for (i, axis) in ["x", "y", "z"].iter().enumerate() {
                    ui.label(*axis);
                    ui.add(egui::DragValue::new(&mut target[i]).speed(10.0));
                }
            });
            ui.end_row();
            if target != [focus.target.x, focus.target.y, focus.target.z] {
                // A dialled-in target is a request like any other: it goes through
                // the same path a frame request does. The heading is kept, so the
                // camera swings round to the new point rather than snapping to a
                // fixed angle.
                let distance = focus.distance;
                crate::viewport::request_view(
                    &mut focus,
                    crate::viewport::ViewPose {
                        target: Vec3::new(target[0], target[1], target[2]),
                        distance,
                        pitch: None,
                        yaw: None,
                    },
                );
            }

            // The distance is how far the camera sits from what it is looking at.
            // A cell sits tens of thousands of units from the origin, so the value
            // is shown to a unit rather than the default two decimals.
            let mut distance = focus.distance;
            ui.label("distance");
            ui.add(
                egui::DragValue::new(&mut distance)
                    .speed(10.0)
                    .range(20.0..=200_000.0),
            );
            ui.end_row();
            if (distance - focus.distance).abs() > f32::EPSILON {
                // A dialled-in distance keeps the target and only closes or opens
                // the gap between the camera and what it is looking at.
                let target = focus.target;
                crate::viewport::request_view(
                    &mut focus,
                    crate::viewport::ViewPose {
                        target,
                        distance,
                        pitch: None,
                        yaw: None,
                    },
                );
            }
        });

        ui.separator();
        // Where the camera is, which is what the target and distance imply.
        // Reading it says whether the camera is where you think it is.
        ui.label(format!("eye  {:.0}, {:.0}, {:.0}", eye.x, eye.y, eye.z));
        let yaw = forward.x.atan2(forward.z).to_degrees();
        let pitch = forward.y.asin().to_degrees();
        ui.label(format!("yaw {yaw:.1}°  pitch {pitch:.1}°"));
        ui.label(format!("distance {:.0}", focus.distance));

        ui.separator();
        if ui.button("Reset").clicked() {
            let default = crate::viewport::ViewportFocus::default();
            // Reset is a distance and a heading, not a place: the camera is put
            // the default distance away from where it already points, facing level
            // along its own yaw. It is the same pose a fresh cell opens with.
            let (pitch, yaw) = (forward.y.asin(), forward.x.atan2(forward.z));
            crate::viewport::request_view(
                &mut focus,
                crate::viewport::ViewPose {
                    target: eye + forward * default.distance,
                    distance: default.distance,
                    pitch: Some(pitch),
                    yaw: Some(yaw),
                },
            );
        }
    });
}
