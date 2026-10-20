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

/// The "new cell" and "new worldspace" controls.
///
/// GECK's Cell View has a "Create" button that makes a new cell in the
/// current worldspace. This is the same: two buttons that open a form, and
/// the form that stages the cell or worldspace for the next save.
///
/// The buttons sit at the **top** of the Cells window, next to the scope
/// dropdown, because the cell list below is a scroll area that fills the
/// window — anything after it is pushed out of sight.
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

    // The two buttons, in a row.
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
                filter.is_empty() || row_matches(r, &details, &filter)
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
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("references_table")
                .num_columns(5)
                .striped(true)
                .min_col_width(50.0)
                .show(ui, |ui| {
                    ui.strong("Editor ID");
                    ui.strong("Form ID");
                    ui.strong("Type");
                    ui.strong("Owner");
                    ui.strong("Lock");
                    ui.end_row();
                    for r in &rows {
                        let open = session.selected == Some(r.form_id);
                        let base = r.base.as_deref().and_then(parse_formid);
                        // The Editor ID cell is the clickable one, so a row is
                        // selected the way the old list was: click the name.
                        //
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
                        if ui.selectable_label(open, text).clicked() {
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
                        ui.label(egui::RichText::new(format!("{:08X}", r.form_id)).monospace());
                        // The type is what the base object is, as GECK names
                        // it — "Static", "Activator". A reference whose base is
                        // missing has nothing to say, so the column stays empty
                        // rather than showing the reference's own signature for
                        // some rows and not others.
                        let type_cell = base
                            .and_then(|b| details.base_type(b))
                            .unwrap_or("")
                            .to_string();
                        ui.label(type_cell);
                        // The owner's name, or the FormID when the record has no
                        // Editor ID to show.
                        let owner_cell = r
                            .owner
                            .and_then(|o| details.owner(o))
                            .unwrap_or("")
                            .to_string();
                        ui.label(owner_cell);
                        let lock_cell = r.lock.map(|l| l.label()).unwrap_or_default();
                        ui.label(egui::RichText::new(lock_cell).monospace());
                        ui.end_row();
                    }
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
fn parse_formid(s: &str) -> Option<u32> {
    u32::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

/// Whether one row's columns contain the filter text, lower-cased.
///
/// The Editor ID, the FormID, the base object's Editor ID and the owner's name
/// are all searched: a cell's references mostly have no Editor ID of their own,
/// so matching only that column would make most rows unfindable.
fn row_matches(
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
        // so the record's borrow ends here.
        let Some(staged) = session.selected_record() else {
            ui.label(format!("{fid:08X} not found"));
            return;
        };
        let rec = staged.record();
        let sig_label = rec.sig_str();
        let rec_sig = rec.header.sig;
        let pos = crate::session::position_of(rec);
        let views = sub_views(&session.schema, rec).unwrap_or_default();
        // A pending edit is not applied to the record in the document, so the
        // grid reads the queued bytes to show what the field will become.
        let pending: std::collections::HashMap<[u8; 4], Vec<u8>> = session
            .doc
            .edits_for(fid)
            .unwrap_or_default()
            .iter()
            .filter_map(|e| match e {
                nvre_edit::document::Edit::SetSub { sig, data } => Some((*sig, data.clone())),
                _ => None,
            })
            .collect();
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
        egui::ScrollArea::vertical().show(ui, |ui| {
            for s in &views {
                // A subrecord's signature is a `String` in the view, and a
                // `[u8; 4]` everywhere the schema and the document want it. It
                // is parsed once here and used for every lookup below.
                let sig = sig4(&s.sig);
                // The value the grid edits: the queued bytes when a field of this
                // subrecord is pending, else the record's own.
                let shown = match pending.get(&sig) {
                    Some(bytes) => nvre_edit::encode::decoded(
                        &session.schema,
                        &rec_sig,
                        &fnv_esm::Subrecord {
                            sig,
                            data: bytes.clone(),
                            extended: false,
                        },
                    )
                    .unwrap_or_else(|| s.value.clone()),
                    None => s.value.clone(),
                };
                ui.horizontal(|ui| {
                    ui.strong(&s.name);
                    ui.monospace(&s.sig);
                    if pending.contains_key(&sig) {
                        ui.colored_label(egui::Color32::LIGHT_BLUE, "edited");
                    } else if !s.writable {
                        ui.colored_label(egui::Color32::YELLOW, "read-only");
                    }
                });
                if let Some(p) = &s.problem {
                    ui.colored_label(egui::Color32::RED, p);
                }
                // One row per field, walked out of the decoded value.
                let fields = nvre_edit::encode::field_names(&session.schema, &rec_sig, &sig);
                if fields.is_empty() {
                    ui.label("(no fields)");
                }
                egui::Grid::new(("fields", &s.sig))
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for path in &fields {
                            let Some(current) = get_path(&shown, path) else {
                                continue;
                            };
                            // A bare scalar subrecord is addressed by the empty
                            // path, so its row says what the value is rather
                            // than showing an empty field name.
                            ui.label(if path.is_empty() { "value" } else { path });
                            field_editor(ui, &mut session, &mut edit, s, &rec_sig, path, current);
                            ui.end_row();
                        }
                    });
                ui.add_space(4.0);
            }
        });
    });
}

/// The value at a dotted path inside a decoded subrecord.
fn get_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut at = value;
    for part in path.split('.') {
        at = at.as_object()?.get(part)?;
    }
    Some(at)
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
fn field_editor(
    ui: &mut egui::Ui,
    session: &mut crate::session::Session,
    edit: &mut FieldEdit,
    view: &nvre_edit::encode::SubView,
    rec_sig: &[u8; 4],
    path: &str,
    current: &serde_json::Value,
) {
    // A field of a read-only subrecord is not editable either: the bytes cannot
    // be written back, so offering a widget would only mislead.
    if !view.writable {
        ui.colored_label(egui::Color32::YELLOW, "read-only");
        return;
    }
    let sig = sig4(&view.sig);
    let mut changed: Option<serde_json::Value> = None;
    match current {
        serde_json::Value::Number(n) => {
            let mut v = n.as_f64().unwrap_or(0.0);
            let is_int = n.is_i64() || n.is_u64();
            let drag = egui::DragValue::new(&mut v).speed(if is_int { 1.0 } else { 0.1 });
            if ui.add(drag).changed() {
                changed = Some(if is_int {
                    serde_json::json!(v.round() as i64)
                } else {
                    serde_json::json!(v)
                });
            }
        }
        serde_json::Value::String(s) => {
            let mut text = s.clone();
            if ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(120.0))
                .changed()
            {
                changed = Some(serde_json::json!(text));
            }
        }
        // A flags field decodes to `{"value": n, "set": [names], "unknown_bits":
        // hex}`: the raw number, the names of the bits that are on, and any bit
        // the schema does not name. Each named bit becomes a checkbox, and the
        // bits the schema does not name are carried through untouched — a field
        // the editor cannot explain still round-trips byte-exact.
        serde_json::Value::Object(o) => {
            let bits = flag_bits(&session.schema, rec_sig, &view.sig);
            if !bits.is_empty() {
                let raw = o.get("value").and_then(|v| v.as_u64()).unwrap_or(0);
                let unknown = o
                    .get("unknown_bits")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let mut next = raw;
                let mut touched = false;
                for (bit, name) in &bits {
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
        // Arrays and null have no single-value widget: an array's length is
        // part of its encoding, so editing one element in place would silently
        // drop the rest. Shown as text, and the subrecord's raw JSON stays
        // available in the value below.
        other => {
            ui.monospace(match other {
                serde_json::Value::Array(_) => "(list)",
                serde_json::Value::Null => "(none)",
                _ => "",
            });
        }
    }
    if let Some(value) = changed {
        edit.last = Some((sig, path.to_string()));
        match session.set_selected_field(&sig, path, value) {
            Ok(()) => {}
            Err(e) => edit.error = Some(e),
        }
    }
}

/// The bits a subrecord's schema names, as `(bit, name)` in bit order.
///
/// The decoder emits a flags field as `{"value": n, "set": [names], ...}` and
/// drops the bit numbers, so the only place they exist is the schema's
/// `{"flags": prim, "bits": {...}}` table. Reading them back from there is what
/// makes a checkbox write the right bit: guessing a bit from a name's position
/// would silently corrupt every flags field whose bits are not declared in
/// ascending order from zero.
fn flag_bits(schema: &fnv_esm::Schema, rec_sig: &[u8; 4], sub_sig: &str) -> Vec<(u32, String)> {
    let Some(def) = schema.lookup(rec_sig, &sig4(sub_sig)) else {
        return Vec::new();
    };
    match schema.resolve(&def.ty) {
        fnv_esm::schema::TypeDef::Flags { bits, .. } => bits.clone(),
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

/// How wide the object palette's category column is, in points.
///
/// GECK's Object Window fixes its category pane at about this much, which is
/// enough for "Non-Player Character" and the object count without swallowing the
/// pane a modder actually picks from.
const CATEGORY_WIDTH: f32 = 180.0;

/// The gap between the palette's two columns, wide enough for a separator.
const SEPARATOR_GAP: f32 = 8.0;

/// The object palette: a category list down the left side, that category's
/// objects on the right.
///
/// GECK's Object Window is laid out this way, and it is what makes all game
/// objects navigable: a modder picks "Static" and sees statics rather than
/// scrolling past containers and lights to find them. The categories are grouped
/// under GECK's top-level sections ("Actors", "Items", "WorldObjects", ...), and
/// clicking an object selects it or places it if placeable.
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
                    .desired_width(ui.available_width() - 60.0),
            );
        });
        ui.separator();
        // Cached in the session and rebuilt only when the filter changes: the
        // query walks the load order, which costs hundreds of milliseconds, and
        // the panel draws every frame. Borrowed rather than cloned.
        let cell_open = session.cell.is_some();
        let selected_fid = session.selected;
        if session.object_groups(&filters.place).is_empty() {
            ui.label("no objects match");
            return;
        }
        let groups = session.object_groups(&filters.place);
        // Clamp the selection into range. A filter that narrows to one category
        // selects it, so typing a name does not leave the right half of the
        // window empty behind a category the filter emptied out. A filter that
        // removes categories shifts the indices, so a selection past the new end
        // lands on the last category rather than indexing out of bounds.
        *chosen = (*chosen).min(groups.len().saturating_sub(1));
        // Every count the footer needs is read out before the action is placed,
        // which needs `&mut Session` while `groups` still borrows it.
        let total: usize = groups.iter().map(|g| g.objects.len()).sum();
        let empty = groups.iter().filter(|g| g.objects.is_empty()).count();
        let shown = groups.len() - empty;

        let cur_group = &groups[*chosen];
        let is_placeable = crate::session::is_placeable_sig(&cur_group.sig);
        let mut place_fid: Option<u32> = None;
        let mut open_obj: Option<(u32, [u8; 4])> = None;
        let mut select_fid: Option<u32> = None;

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
            egui::ScrollArea::vertical().show(&mut ui, |ui| {
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
        egui::ScrollArea::vertical().show(&mut ui, |ui| {
            for (fid, label) in &groups[*chosen].objects {
                ui.horizontal(|ui| {
                    if ui.small_button("Edit").clicked() {
                        open_obj = Some((*fid, cur_group.sig));
                    }
                    if is_placeable && cell_open && ui.small_button("Place").clicked() {
                        place_fid = Some(*fid);
                    }
                    let selected = selected_fid == Some(*fid);
                    let resp = ui.selectable_label(selected, label);
                    if resp.double_clicked() {
                        open_obj = Some((*fid, cur_group.sig));
                    } else if resp.clicked() {
                        select_fid = Some(*fid);
                    }
                });
            }
            if groups[*chosen].objects.is_empty() {
                ui.label("no objects in this category match the filter");
            }
        });
        if let Some((fid, sig)) = open_obj {
            session.open_object(fid, sig);
        } else if let Some(fid) = place_fid {
            // Placed at the cell's origin: the camera's target is the better
            // default once the viewport can report it.
            session.place(fid, [0.0, 0.0, 0.0]);
        } else if let Some(fid) = select_fid {
            session.select(fid);
            session.status = format!("selected {fid:08X}");
        }
        ui.separator();
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
    });
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
