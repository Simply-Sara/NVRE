//! The launcher: pick the plugin to edit and the masters it will use.
//!
//! Nothing is loaded at startup, the way GECK asks which file to work on before
//! it opens anything. The active file is picked from the game's Data folder, and
//! its own masters are read from the file's TES4 header and loaded without being
//! asked for — the plugin already names them, so making the user pick them again
//! would be a chance to get the load order wrong.
//!
//! Extra masters can be added on top: a plugin under construction often needs a
//! master the file does not name yet, and the saved plugin records whatever is
//! in the list when it is written.

use bevy::prelude::*;
use bevy_egui::egui;

/// What the launcher is doing, and what it has chosen.
#[derive(Resource, Default)]
pub struct Launcher {
    /// The plugin to edit, chosen with the file dialog.
    pub active: Option<std::path::PathBuf>,
    /// The active file's own masters, read from its header.
    pub required: Vec<String>,
    /// Every master in the load order: the required ones, plus any added.
    pub masters: Vec<String>,
    /// A master the user is typing in, to add to the list.
    pub adding: String,
    /// What the last attempt to load said.
    pub error: Option<String>,
    /// The plugins found in the Data folder, and the folder they came from.
    listing: Vec<std::path::PathBuf>,
    listing_dir: Option<std::path::PathBuf>,
}

/// The plugins in a folder, sorted by name.
///
/// Only `.esm`, `.esp` and `.esl` files are listed: those are the three the
/// format handles, and anything else in a Data folder is not loadable.
fn list_plugins(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|x| {
                    x.eq_ignore_ascii_case("esm")
                        || x.eq_ignore_ascii_case("esp")
                        || x.eq_ignore_ascii_case("esl")
                })
                .unwrap_or(false)
        })
        .collect();
    out.sort();
    out
}

/// Draw the launcher in the main window.
///
/// The window is the whole app until a file is chosen: the editor's systems do
/// not run until then, so there is nothing behind it to interact with. The
/// contents therefore go in a [`egui::CentralPanel`], which is the window
/// itself, rather than an [`egui::Window`] drawn inside it — a window in a
/// window gives a second title bar, its own margin and a drag handle, all
/// framing the same OS frame again.
pub fn ui(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<bevy_egui::PrimaryEguiContext>>,
    mut launcher: ResMut<Launcher>,
    data: Res<crate::DataDir>,
    mut open: MessageWriter<crate::OpenRequested>,
    failed: Option<Res<crate::OpenFailed>>,
) {
    // The main window is the frame, so the contents fill it: the window itself
    // rather than a window inside the window.
    let ctx = ctx.get_mut();

    // The plugins in the Data folder, listed by name. Reading the folder is
    // cheap and the list changes rarely, so it is cached per folder.
    if launcher.listing_dir.as_deref() != Some(data.0.as_path()) {
        launcher.listing_dir = Some(data.0.clone());
        launcher.listing = list_plugins(&data.0);
    }

    // The listing is read out once: the loop below needs `launcher` mutably to
    // change the selection.
    let listing = launcher.listing.clone();

    egui::CentralPanel::default().show(&mut crate::panels::root_ui(ctx, "launcher"), |ui| {
        ui.heading("New Vegas Rust Editor");
        ui.label("Pick the file to edit. Its masters are loaded automatically.");
        ui.separator();

        ui.columns(2, |cols| {
            // The files, on the left.
            cols[0].label("Files");
            egui::ScrollArea::vertical()
                .id_salt("files")
                .max_height(250.0)
                .show(&mut cols[0], |ui| {
                    for p in &listing {
                        let name = p
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let selected = launcher.active.as_deref() == Some(p.as_path());
                        if ui.selectable_label(selected, &name).clicked() {
                            launcher.active = Some(p.clone());
                            launcher.error = None;
                            // The file's own masters, read as soon as it is
                            // picked: the user is not asked for them.
                            launcher.required = read_masters_for(p);
                            launcher.masters = launcher.required.clone();
                        }
                    }
                });

            // The masters, on the right.
            cols[1].label("Masters");
            if launcher.required.is_empty() {
                cols[1].label("this file names no masters");
            }
            // The required masters are shown as they are: removing one would
            // change what the file means, so the list is fixed and additions
            // go after it.
            for m in &launcher.required {
                cols[1].label(format!("{m} (from the file)"));
            }
            // The count is read out first, to avoid borrowing twice.
            let required_count = launcher.required.len();
            let extra: Vec<String> = launcher
                .masters
                .iter()
                .skip(required_count)
                .cloned()
                .collect();
            for (i, m) in extra.iter().enumerate() {
                cols[1].horizontal(|ui| {
                    ui.label(m);
                    if ui.small_button("remove").clicked() {
                        launcher.masters.remove(required_count + i);
                    }
                });
            }
            cols[1].horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut launcher.adding).hint_text("add a master"),
                );
                let add = ui.button("Add").clicked()
                    || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if add && !launcher.adding.trim().is_empty() {
                    let name = launcher.adding.trim().to_string();
                    if !launcher.masters.iter().any(|m| m == &name) {
                        launcher.masters.push(name);
                    }
                    launcher.adding.clear();
                }
            });
        });

        if let Some(e) = &launcher.error {
            ui.colored_label(egui::Color32::RED, e);
        }
        if let Some(f) = &failed {
            ui.colored_label(egui::Color32::RED, &f.0);
        }

        ui.separator();
        let ready = launcher.active.is_some();
        ui.add_enabled_ui(ready, |ui| {
            if ui.button("Open").clicked() {
                // The session is built by `open_selected` in response to
                // this; doing it here would mean holding a borrow of the
                // world open across a file read.
                open.write(crate::OpenRequested);
            }
        });
    });
}

/// The masters a plugin's own header names.
///
/// Read straight from the file, so the launcher can show them before anything is
/// loaded. A file that cannot be read has no masters to show, which the caller
/// reports as an empty list rather than an error: the load itself will say why.
pub fn read_masters_for(path: &std::path::Path) -> Vec<String> {
    fnv_esm::Plugin::open(path)
        .map(|p| p.header.masters.iter().map(|m| m.name.clone()).collect())
        .unwrap_or_default()
}
