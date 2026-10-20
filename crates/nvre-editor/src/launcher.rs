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

impl Launcher {
    /// A fresh launcher for `game`.
    ///
    /// The default master list is the game's own load order, so a Fallout 3
    /// session starts with `Fallout3.esm` rather than FNV's master — but the
    /// resource is only defaulted once at startup, which is when the game is
    /// known.
    pub fn for_game(game: &crate::game::GameProfile) -> Launcher {
        Launcher {
            tab: LauncherTab::OpenExisting,
            active: None,
            required: Vec::new(),
            masters: game.default_masters(),
            new_name: "NewPlugin.esp".to_string(),
            new_is_master: false,
            adding: String::new(),
            error: None,
            listing: Vec::new(),
            listing_dir: None,
        }
    }
}

impl Default for Launcher {
    fn default() -> Self {
        Launcher::for_game(&crate::game::FNV)
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

/// Which game the launcher is pointed at, and which one the editor is using.
///
/// `chosen` is the user's pick from the dropdown; it starts as the detected game
/// so the launcher is right without a click. `applied` is the game the running
/// editor actually is: changing the pick does not switch the editor until Apply
/// is pressed, because switching re-opens the asset layer.
#[derive(Resource)]
pub struct GameChoice {
    /// The game the user has selected in the launcher.
    pub chosen: &'static crate::game::GameProfile,
    /// The game the editor is running as, until Apply is pressed.
    pub applied: &'static crate::game::GameProfile,
    /// Set when a game needs its Data folder pointed somewhere else first.
    pub data_override: Option<std::path::PathBuf>,
}

impl Default for GameChoice {
    fn default() -> Self {
        GameChoice {
            chosen: &crate::game::FNV,
            applied: &crate::game::FNV,
            data_override: None,
        }
    }
}

impl GameChoice {
    /// A fresh choice for `game`, with nothing pending.
    pub fn new(game: &'static crate::game::GameProfile) -> GameChoice {
        GameChoice {
            chosen: game,
            applied: game,
            data_override: None,
        }
    }

    /// True when the pick differs from the running game.
    pub fn pending(&self) -> bool {
        !std::ptr::eq(self.chosen, self.applied)
    }
}

/// Draw the launcher in the main window.
pub fn ui(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<bevy_egui::PrimaryEguiContext>>,
    mut launcher: ResMut<Launcher>,
    data: Res<crate::DataDir>,
    mut choice: ResMut<GameChoice>,
    mut open: MessageWriter<crate::OpenRequested>,
    failed: Option<Res<crate::OpenFailed>>,
) {
    let ctx = ctx.get_mut();

    // The plugins in the Data folder, listed by name. The folder is the one
    // the picker names, which is the game's own when no override is set.
    let active_dir = choice
        .data_override
        .clone()
        .unwrap_or_else(|| data.0.clone());
    if launcher.listing_dir.as_deref() != Some(active_dir.as_path()) {
        launcher.listing_dir = Some(active_dir.clone());
        launcher.listing = list_plugins(&active_dir);
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
                ui.heading("Select a game");

                // The game picker: what to mod, and where its data lives.
                ui.horizontal(|ui| {
                    ui.label("Game:");
                    let selected = choice.chosen;
                    egui::ComboBox::from_id_salt("game_pick")
                        .selected_text(selected.name)
                        .show_ui(ui, |ui| {
                            for g in &crate::game::ALL {
                                ui.selectable_value(
                                    &mut choice.chosen,
                                    g,
                                    format!("{} ({})", g.name, g.master),
                                );
                            }
                        });
                    if choice.pending() {
                        if ui.button("Apply").clicked() {
                            choice.applied = choice.chosen;
                            // The plugin list and master defaults are the game's.
                            launcher.listing_dir = None;
                            launcher.listing.clear();
                            launcher.active = None;
                            launcher.required.clear();
                            launcher.masters = choice.applied.default_masters();
                        }
                    } else {
                        ui.label(format!("editing {}", choice.applied.name));
                    }
                });

                // The Data folder: detected per game, overridable when several
                // games are installed and the folder named on the command line
                // is not the picked game's.
                let mut folder = choice
                    .data_override
                    .clone()
                    .unwrap_or_else(|| data.0.clone())
                    .to_string_lossy()
                    .into_owned();
                ui.horizontal(|ui| {
                    ui.label("Data folder:");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut folder)
                            .desired_width(360.0)
                            .hint_text("/path/to/<Game>/Data"),
                    );
                    if resp.lost_focus() {
                        choice.data_override = if folder.trim().is_empty() {
                            None
                        } else {
                            Some(std::path::PathBuf::from(folder.trim()))
                        };
                    }
                });
                let active_dir = choice
                    .data_override
                    .clone()
                    .unwrap_or_else(|| data.0.clone());
                if !active_dir.join(choice.chosen.master).exists() {
                    ui.colored_label(
                        egui::Color32::RED,
                        format!("no {} in {}", choice.chosen.master, active_dir.display()),
                    );
                }
                // What the picked game cannot do yet, said plainly here rather
                // than shown as an empty viewport that looks like a bug.
                for note in choice.chosen.notes {
                    ui.colored_label(egui::Color32::from_rgb(0xd0, 0xa0, 0x40), *note);
                }

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
