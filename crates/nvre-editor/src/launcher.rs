//! The launcher: pick the plugin to edit, create a new plugin, and configure masters.

use bevy::prelude::*;
use bevy_egui::egui;

/// Which tab the launcher is currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LauncherTab {
    #[default]
    OpenExisting,
    CreateNew,
}

/// What the launcher is doing, and what it has chosen.
#[derive(Resource)]
pub struct Launcher {
    /// Active tab (open existing or create new).
    pub tab: LauncherTab,
    /// The plugin to edit, chosen with the file list.
    pub active: Option<std::path::PathBuf>,
    /// The active file's own masters, read from its header.
    pub required: Vec<String>,
    /// Every master in the load order: the required ones, plus any added.
    pub masters: Vec<String>,
    /// For creating a new plugin: file name.
    pub new_name: String,
    /// For creating a new plugin: true for .esm (Master), false for .esp (Plugin).
    pub new_is_master: bool,
    /// A master the user is typing in, to add to the list.
    pub adding: String,
    /// What the last attempt to load said.
    pub error: Option<String>,
    /// The plugins found in the Data folder, and the folder they came from.
    pub listing: Vec<std::path::PathBuf>,
    pub listing_dir: Option<std::path::PathBuf>,
}

impl Default for Launcher {
    fn default() -> Self {
        Launcher {
            tab: LauncherTab::OpenExisting,
            active: None,
            required: Vec::new(),
            masters: vec!["FalloutNV.esm".to_string()],
            new_name: "NewPlugin.esp".to_string(),
            new_is_master: false,
            adding: String::new(),
            error: None,
            listing: Vec::new(),
            listing_dir: None,
        }
    }
}

/// The plugins in a folder, sorted by name.
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
pub fn ui(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<bevy_egui::PrimaryEguiContext>>,
    mut launcher: ResMut<Launcher>,
    data: Res<crate::DataDir>,
    mut open: MessageWriter<crate::OpenRequested>,
    failed: Option<Res<crate::OpenFailed>>,
) {
    let ctx = ctx.get_mut();

    // The plugins in the Data folder, listed by name.
    if launcher.listing_dir.as_deref() != Some(data.0.as_path()) {
        launcher.listing_dir = Some(data.0.clone());
        launcher.listing = list_plugins(&data.0);
    }

    let listing = launcher.listing.clone();

    egui::CentralPanel::default().show(&mut crate::panels::root_ui(ctx, "launcher"), |ui| {
        // Lay out bottom bar anchored to the bottom of the window
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(14.0);

            // Action button anchored at the bottom
            ui.horizontal(|ui| {
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| match launcher.tab {
                        LauncherTab::OpenExisting => {
                            let ready = launcher.active.is_some();
                            ui.add_enabled_ui(ready, |ui| {
                                let label = if let Some(p) = &launcher.active {
                                    format!(
                                        "Open {}",
                                        p.file_name()
                                            .map(|n| n.to_string_lossy())
                                            .unwrap_or_default()
                                    )
                                } else {
                                    "Open".to_string()
                                };
                                if ui
                                    .add_sized([160.0, 32.0], egui::Button::new(label))
                                    .clicked()
                                {
                                    open.write(crate::OpenRequested);
                                }
                            });
                        }
                        LauncherTab::CreateNew => {
                            let ready = !launcher.new_name.trim().is_empty()
                                && !launcher.masters.is_empty();
                            ui.add_enabled_ui(ready, |ui| {
                                let label = if launcher.new_is_master {
                                    format!("Create & Open Master ({})", launcher.new_name.trim())
                                } else {
                                    format!("Create & Open Plugin ({})", launcher.new_name.trim())
                                };
                                if ui
                                    .add_sized([220.0, 32.0], egui::Button::new(label))
                                    .clicked()
                                {
                                    open.write(crate::OpenRequested);
                                }
                            });
                        }
                    },
                );
            });

            if let Some(e) = &launcher.error {
                ui.colored_label(egui::Color32::RED, e);
            }
            if let Some(f) = &failed {
                ui.colored_label(egui::Color32::RED, &f.0);
            }

            ui.separator();

            // Main body fills all remaining space above the bottom bar
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                // Top Header
                ui.heading("New Vegas Rust Editor");

                // Top tab switcher
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(
                            launcher.tab == LauncherTab::OpenExisting,
                            "Open Existing Plugin",
                        )
                        .clicked()
                    {
                        launcher.tab = LauncherTab::OpenExisting;
                    }
                    if ui
                        .selectable_label(
                            launcher.tab == LauncherTab::CreateNew,
                            "Create New Plugin (.esp / .esm)",
                        )
                        .clicked()
                    {
                        launcher.tab = LauncherTab::CreateNew;
                    }
                });
                ui.separator();

                match launcher.tab {
                    LauncherTab::OpenExisting => {
                        ui.label(
                            "Select a file to edit. Required masters are checked automatically.",
                        );
                        ui.add_space(4.0);

                        let scroll_height = (ui.available_height() - 40.0).max(180.0);

                        ui.columns(2, |cols| {
                            // Files on left
                            cols[0].label("Files in Data:");
                            egui::ScrollArea::vertical()
                                .id_salt("files_open")
                                .max_height(scroll_height)
                                .show(&mut cols[0], |ui| {
                                    for p in &listing {
                                        let name = p
                                            .file_name()
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or_default();
                                        let selected =
                                            launcher.active.as_deref() == Some(p.as_path());
                                        if ui.selectable_label(selected, &name).clicked() {
                                            launcher.active = Some(p.clone());
                                            launcher.error = None;
                                            launcher.required = read_masters_for(p);
                                            launcher.masters = launcher.required.clone();
                                        }
                                    }
                                });

                            // Masters on right with checkboxes
                            cols[1].label("Masters (Checkboxes):");
                            egui::ScrollArea::vertical()
                                .id_salt("masters_open")
                                .max_height(scroll_height - 35.0)
                                .show(&mut cols[1], |ui| {
                                    for p in &listing {
                                        let name = p
                                            .file_name()
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or_default();
                                        let is_esm =
                                            name.ends_with(".esm") || name.ends_with(".ESM");
                                        if !is_esm && !launcher.masters.contains(&name) {
                                            continue;
                                        }
                                        let is_required = launcher.required.contains(&name);
                                        let mut checked = launcher.masters.contains(&name);

                                        let label = if is_required {
                                            format!("{name} (required)")
                                        } else {
                                            name.clone()
                                        };

                                        let resp = ui.add_enabled(
                                            !is_required,
                                            egui::Checkbox::new(&mut checked, label),
                                        );
                                        if resp.changed() {
                                            if checked {
                                                if !launcher.masters.contains(&name) {
                                                    launcher.masters.push(name);
                                                }
                                            } else {
                                                launcher.masters.retain(|m| m != &name);
                                            }
                                        }
                                    }
                                });

                            // Add custom master text box
                            cols[1].horizontal(|ui| {
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut launcher.adding)
                                        .hint_text("add custom master..."),
                                );
                                let add = ui.button("Add").clicked()
                                    || (resp.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                if add && !launcher.adding.trim().is_empty() {
                                    let name = launcher.adding.trim().to_string();
                                    if !launcher.masters.contains(&name) {
                                        launcher.masters.push(name);
                                    }
                                    launcher.adding.clear();
                                }
                            });
                        });
                    }
                    LauncherTab::CreateNew => {
                        ui.label("Configure and create a new plugin file:");
                        ui.add_space(4.0);

                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("File Name:");
                                let prev_is_master = launcher.new_is_master;
                                ui.text_edit_singleline(&mut launcher.new_name);

                                if ui
                                    .radio_value(
                                        &mut launcher.new_is_master,
                                        false,
                                        ".esp (Plugin)",
                                    )
                                    .clicked()
                                    && prev_is_master
                                    && launcher.new_name.ends_with(".esm")
                                {
                                    launcher.new_name =
                                        launcher.new_name.trim_end_matches(".esm").to_string()
                                            + ".esp";
                                }
                                if ui
                                    .radio_value(&mut launcher.new_is_master, true, ".esm (Master)")
                                    .clicked()
                                    && !prev_is_master
                                    && launcher.new_name.ends_with(".esp")
                                {
                                    launcher.new_name =
                                        launcher.new_name.trim_end_matches(".esp").to_string()
                                            + ".esm";
                                }
                            });
                        });

                        ui.add_space(6.0);
                        ui.label("Select Masters to depend on (Checkboxes):");

                        let scroll_height = (ui.available_height() - 40.0).max(180.0);

                        egui::ScrollArea::vertical()
                            .id_salt("masters_new")
                            .max_height(scroll_height - 35.0)
                            .show(ui, |ui| {
                                for p in &listing {
                                    let name = p
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_default();
                                    let is_esm = name.ends_with(".esm") || name.ends_with(".ESM");
                                    if !is_esm && !launcher.masters.contains(&name) {
                                        continue;
                                    }
                                    let mut checked = launcher.masters.contains(&name);
                                    if ui.checkbox(&mut checked, &name).changed() {
                                        if checked {
                                            if !launcher.masters.contains(&name) {
                                                launcher.masters.push(name);
                                            }
                                        } else {
                                            launcher.masters.retain(|m| m != &name);
                                        }
                                    }
                                }
                            });

                        ui.horizontal(|ui| {
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut launcher.adding)
                                    .hint_text("add custom master..."),
                            );
                            let add = ui.button("Add").clicked()
                                || (resp.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                            if add && !launcher.adding.trim().is_empty() {
                                let name = launcher.adding.trim().to_string();
                                if !launcher.masters.contains(&name) {
                                    launcher.masters.push(name);
                                }
                                launcher.adding.clear();
                            }
                        });
                    }
                }
            });
        });
    });
}

/// The masters a plugin's own header names.
pub fn read_masters_for(path: &std::path::Path) -> Vec<String> {
    fnv_esm::Plugin::open(path)
        .map(|p| p.header.masters.iter().map(|m| m.name.clone()).collect())
        .unwrap_or_default()
}
