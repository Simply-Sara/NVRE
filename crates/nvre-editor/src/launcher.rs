//! The launcher: pick the plugin to edit, create a new plugin, and configure masters.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::plugin_order::{self, PluginOrder};

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
    /// What order the two lists are shown in, and what order a master list
    /// picked from a file is in.
    ///
    /// Only the *master* list follows an order file. The file list is a
    /// listing, and a plugin the order file has never heard of would be pushed
    /// to its end by an order it is not part of; the order the game loads
    /// masters in is the one that decides whose record wins, so that list is
    /// the one worth ordering.
    pub order: PluginOrder,
    /// For creating a new plugin: file name.
    pub new_name: String,
    /// For creating a new plugin: true for .esm (Master), false for .esp (Plugin).
    pub new_is_master: bool,
    /// For creating a new plugin: the author and description the header carries.
    ///
    /// Both are buffers rather than the committed value, because egui rebuilds
    /// the field every frame and a value reconstructed from elsewhere discards
    /// every keystroke.
    pub new_author: String,
    pub new_description: String,
    /// A master the user is typing in, to add to the list.
    pub adding: String,
    /// What the last attempt to load said.
    pub error: Option<String>,
    /// The plugins found in the Data folder, and the folder they came from.
    pub listing: Vec<std::path::PathBuf>,
    /// What `listing` was built from: the folder, and the order it was read
    /// with. The launcher draws every frame, so the folder is listed and the
    /// order applied only when one of these changes.
    pub listing_key: Option<(std::path::PathBuf, PluginOrder)>,
    /// The order file the current order was read from, when it was found.
    pub order_file: Option<std::path::PathBuf>,
    /// Why the order file could not be used, for the launcher to say.
    ///
    /// An order the user asked for and did not get has to say so: a list in
    /// file-name order looks exactly like an order file that was read and
    /// happened to agree.
    pub order_note: Option<String>,
}

impl Launcher {
    /// A fresh launcher for `game`, whose masters are read from `data_dir`.
    ///
    /// The master list is the game's own Data folder, not a fixed table: a
    /// partial install gives only the masters it holds, and a mod master the
    /// user has installed comes along. `for_game` is called once at startup,
    /// which is when the game is known.
    pub fn for_game(game: &crate::game::GameProfile) -> Launcher {
        let dir = game.data_dir().unwrap_or_default();
        Launcher {
            tab: LauncherTab::OpenExisting,
            active: None,
            required: Vec::new(),
            masters: game.masters_in(&dir),
            order: PluginOrder::Name,
            new_name: "NewPlugin.esp".to_string(),
            new_is_master: false,
            // Empty, so the header gets GECK's `DEFAULT` author and no
            // description unless the user types one.
            new_author: String::new(),
            new_description: String::new(),
            adding: String::new(),
            error: None,
            listing: Vec::new(),
            listing_key: None,
            order_file: None,
            order_note: None,
        }
    }

    /// Point at `game`'s Data folder: re-list its plugins and re-read its order.
    ///
    /// Called when the game or the Data folder changes. The order stays the
    /// user's pick — a game switch is not a reason to go back to file-name
    /// order — but it is re-read from the new folder, which may have no such
    /// file.
    ///
    /// `None` is a game with no folder yet: nothing is listed and no master is
    /// named, which is the state the launcher says so in rather than one it
    /// guesses at. That is spelled out here rather than left to `read_dir` on an
    /// empty path failing, which happens to give the same answer by accident.
    pub fn retarget(&mut self, game: &crate::game::GameProfile, dir: Option<&std::path::Path>) {
        self.listing_key = None;
        self.listing.clear();
        self.active = None;
        self.required.clear();
        self.order_file = None;
        self.order_note = None;
        let Some(dir) = dir else {
            self.masters.clear();
            return;
        };
        self.refresh_listing(game, dir);
    }

    /// Re-list the folder and re-read the order, when either has changed.
    ///
    /// The launcher draws every frame, so this is called every frame and does
    /// the work only when the key moves.
    pub fn refresh_listing(&mut self, game: &crate::game::GameProfile, dir: &std::path::Path) {
        let key = (dir.to_path_buf(), self.order);
        if self.listing_key.as_ref() == Some(&key) {
            return;
        }
        self.listing_key = Some(key);
        self.listing = list_plugins(dir);

        let Some(file) = plugin_order::read(dir, self.order) else {
            // No order file: the list is the folder, and the master list is the
            // one the headers give.
            self.order_file = None;
            self.order_note = self
                .order
                .file()
                .map(|f| format!("no {f} in {} — listed by file name", dir.display()));
            self.masters = game.masters_in(dir);
            return;
        };
        self.order_file = Some(file.path);
        self.order_note = None;
        let rank = plugin_order::Rank::of(&file.names);
        plugin_order::order_paths(&mut self.listing, &rank);
        // The masters are re-derived with the file's order rather than re-sorted
        // into it: the headers constrain the order and the file only says where
        // it has a preference, so a master the file puts after something that
        // requires it stays where the headers put it. Re-sorting the result
        // instead would produce a list `LoadOrder::new` rejects.
        self.masters = game.masters_in_with(dir, &rank);
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

/// Which game the launcher is pointed at, and where its data lives.
///
/// One field, not two. There is nothing to stage a pick against: nothing is
/// built from the game until Open is pressed, and the launcher's own lists are
/// re-read from the folder on the spot. A `chosen`/`applied` pair with an Apply
/// button between them only creates the trap of picking one game and opening
/// another.
#[derive(Resource)]
pub struct GameChoice {
    /// The game selected in the dropdown. Starts as the detected game, so the
    /// launcher is right without a click.
    pub game: &'static crate::game::GameProfile,
    /// The Data folder in use. Starts as the game's Steam install when it has
    /// one, and as nothing when it does not — the user fills it in. Held here
    /// rather than derived every frame, because it is what the editor opened
    /// with and must not change under it.
    pub data_dir: Option<std::path::PathBuf>,
    /// The Data-folder field's text, live.
    ///
    /// An egui widget's buffer must outlive the frame: rebuilding it from
    /// `data_dir` each frame throws away whatever was just typed, so the field
    /// looks editable and silently is not. This holds the text between frames;
    /// `data_dir` is only written when the edit is committed.
    pub folder_edit: String,
}

impl GameChoice {
    /// A fresh choice for `game`, pointing at its Steam install if it has one.
    ///
    /// A game with no install found starts with no path: the launcher shows the
    /// field empty for the user to fill, which is the honest state rather than a
    /// guess.
    ///
    /// The pick is remembered, so the next run starts on the same game without
    /// being told which one to use.
    pub fn new(game: &'static crate::game::GameProfile) -> GameChoice {
        let data_dir = game.data_dir();
        let folder_edit = data_dir
            .clone()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        GameChoice {
            game,
            data_dir,
            folder_edit,
        }
    }

    /// Point at `game`, re-resolving its Steam install.
    pub fn choose(&mut self, game: &'static crate::game::GameProfile) {
        self.game = game;
        self.data_dir = game.data_dir();
        // The field shows the new folder, so the buffer follows it.
        self.folder_edit = self
            .data_dir
            .clone()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
    }

    /// The Data folder in use: the game's own install, or the folder the user
    /// typed into the field. A game with no install and no typed path has no
    /// folder, which the caller reports rather than guessing at.
    pub fn active_dir(&self) -> Option<std::path::PathBuf> {
        self.data_dir.clone()
    }

    /// Point the field at `dir`, as the folder picker's answer.
    ///
    /// The text the picker returns is committed through the same path as a
    /// typed one — [`GameChoice::commit_folder`] — so a browsed folder and a
    /// typed one cannot end up stored differently. The field is set first
    /// because that is the value `commit_folder` reads.
    pub fn pick_folder(&mut self, dir: &std::path::Path) {
        self.folder_edit = dir.to_string_lossy().into_owned();
        self.commit_folder();
    }

    /// Commit the field's text as the Data folder, and remember it.
    ///
    /// An empty field clears the override rather than keeping the last one, so
    /// the game falls back to its Steam install. The result is written to the
    /// config file, which is what keeps the user from typing it again.
    pub fn commit_folder(&mut self) {
        let text = self.folder_edit.trim().to_string();
        self.data_dir = if text.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(text))
        };
        if let Some(dir) = &self.data_dir {
            crate::game::save_data_dir(self.game.id, dir);
        }
    }
}

impl Default for GameChoice {
    fn default() -> Self {
        GameChoice::new(&crate::game::FNV)
    }
}

/// Draw the launcher in the main window.
pub fn ui(
    mut ctx: Single<&mut bevy_egui::EguiContext, With<bevy_egui::PrimaryEguiContext>>,
    mut launcher: ResMut<Launcher>,
    mut choice: ResMut<GameChoice>,
    mut open: MessageWriter<crate::OpenRequested>,
    failed: Option<Res<crate::OpenFailed>>,
) {
    let ctx = ctx.get_mut();

    // Which games Steam has an install for, probed once: the dropdown is redrawn
    // every frame and a probe is a directory stat per library.
    let mut installed: std::collections::HashMap<&str, bool> = std::collections::HashMap::new();

    // The plugins in the Data folder, listed by name. The folder is the
    // picker's: the game's own install, or the one typed into the field. A game
    // with no folder yet lists nothing until one is named.
    let active_dir = choice.active_dir();
    if let Some(dir) = &active_dir {
        launcher.refresh_listing(choice.game, dir);
    }

    let listing = launcher.listing.clone();
    let order_note = launcher.order_note.clone();
    let order_file = launcher.order_file.clone();

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
                            // A name is all a new plugin needs. Zero masters is a
                            // plugin GECK writes too — `fonv_geck_empty_no_master.esp`
                            // is a TES4 header and nothing else — so an empty master
                            // list is a choice, not an unfinished form.
                            let ready = !launcher.new_name.trim().is_empty();
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

                // The game picker: what to mod. Picking a game re-resolves its
                // Steam install, so the folder field fills in when Steam has it
                // and stays empty when it does not, and the plugin and master
                // lists follow the folder it names.
                ui.horizontal(|ui| {
                    ui.label("Game:");
                    let selected = choice.game;
                    egui::ComboBox::from_id_salt("game_pick")
                        .selected_text(selected.name)
                        .show_ui(ui, |ui| {
                            for g in &crate::game::ALL {
                                // Whether this game's Data folder was found is a
                                // disk probe, and a dropdown is redrawn every
                                // frame, so the answer is cached for the
                                // launcher's lifetime.
                                let found = installed
                                    .entry(g.id)
                                    .or_insert_with(|| g.data_dir().is_some());
                                let label = if *found {
                                    format!("{} ({})", g.name, g.master)
                                } else {
                                    format!("{} ({}) — no Data folder found", g.name, g.master)
                                };
                                if ui.selectable_value(&mut choice.game, g, label).clicked()
                                    && !std::ptr::eq(choice.game, selected)
                                {
                                    let picked = choice.game;
                                    choice.choose(picked);
                                    // The plugin list and masters are re-read
                                    // from the new game's own folder: a partial
                                    // install gives only the masters it holds.
                                    launcher.retarget(picked, choice.active_dir().as_deref());
                                }
                            }
                        });
                });

                // The Data folder. Filled from Steam when the install is there;
                // empty for the user to fill when it is not. The buffer lives in
                // the resource so typing is not discarded every frame.
                ui.horizontal(|ui| {
                    ui.label("Data folder:");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut choice.folder_edit)
                            .desired_width(360.0)
                            .hint_text("/path/to/<Game>/Data"),
                    );
                    // The OS's own folder picker, to the right of the field.
                    // A chosen folder is committed on the spot — the picker is
                    // an explicit answer, not a keystroke still being typed —
                    // so the plugin and master lists follow it immediately
                    // rather than waiting for the field to lose focus.
                    if ui.button("Browse…").clicked() {
                        let mut dialog = rfd::FileDialog::new()
                            .set_title(format!("{} — choose the Data folder", choice.game.name));
                        // Start where the field points, when it points
                        // somewhere real: the common case is browsing from the
                        // folder already in use.
                        if let Some(dir) = choice.active_dir().filter(|d| d.is_dir()) {
                            dialog = dialog.set_directory(dir);
                        }
                        if let Some(picked) = dialog.pick_folder() {
                            let before = choice.data_dir.clone();
                            choice.pick_folder(&picked);
                            if choice.data_dir != before {
                                launcher.retarget(choice.game, choice.active_dir().as_deref());
                            }
                        }
                    }
                    // Commit on losing focus or Enter: the plugin list and the
                    // master list both follow the folder.
                    if resp.lost_focus() {
                        let before = choice.data_dir.clone();
                        choice.commit_folder();
                        if choice.data_dir != before {
                            launcher.retarget(choice.game, choice.active_dir().as_deref());
                        }
                    }
                });
                if let Some(dir) = &choice.data_dir {
                    ui.label(format!("{}", dir.display()));
                }
                // A game with no Data folder yet says so, rather than
                // opening a session that reads nothing.
                match choice.active_dir() {
                    Some(dir) => {
                        if !dir.join(choice.game.master).exists() {
                            ui.colored_label(
                                egui::Color32::RED,
                                format!("no {} in {}", choice.game.master, dir.display()),
                            );
                        }
                    }
                    None => {
                        ui.colored_label(
                            egui::Color32::RED,
                            format!("no Data folder for {}: type one above", choice.game.name),
                        );
                    }
                }
                // What the picked game cannot do yet, said plainly here rather
                // than shown as an empty viewport that looks like a bug.
                for note in choice.game.notes {
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

                        // Above the master list it orders, which is the column
                        // on the right.
                        order_picker(
                            ui,
                            &mut launcher,
                            order_file.as_deref(),
                            order_note.as_deref(),
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
                        // The author and description the header carries. GECK's
                        // own Create dialog has no such fields — its plugin is
                        // always CNAM `DEFAULT` with no SNAM — so these are what
                        // let a mod say who wrote it and what it is.
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Author:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut launcher.new_author)
                                        .hint_text("DEFAULT")
                                        .desired_width(ui.available_width() - 20.0),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("Description:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut launcher.new_description)
                                        .hint_text("what this plugin is for")
                                        .desired_width(ui.available_width() - 20.0),
                                );
                            });
                        });

                        ui.add_space(6.0);
                        // Below the author and description and above the master
                        // list, because it is the master list it orders.
                        order_picker(
                            ui,
                            &mut launcher,
                            order_file.as_deref(),
                            order_note.as_deref(),
                        );

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

/// The master-list order drop-down, and what it did.
///
/// Drawn on both tabs, directly above the master list it orders: the order file
/// is read from the Data folder and its order is the one the game loads masters
/// in, so this is also the order a session's load order is built from — the
/// plugin whose definition wins.
///
/// `file` is the order file that was read and `note` why none was. An order the
/// user asked for and did not get has to say so: a list in file-name order looks
/// exactly like a file that was read and agreed.
fn order_picker(
    ui: &mut egui::Ui,
    launcher: &mut Launcher,
    file: Option<&std::path::Path>,
    note: Option<&str>,
) {
    ui.horizontal(|ui| {
        ui.label("Order:");
        let mut picked = launcher.order;
        egui::ComboBox::from_id_salt("order_pick")
            .selected_text(launcher.order.label())
            .show_ui(ui, |ui| {
                for o in PluginOrder::ALL {
                    ui.selectable_value(&mut picked, o, o.label());
                }
            });
        // Written back and acted on next frame: `refresh_listing` re-lists the
        // folder and re-reads the file when the order changes, which is also
        // what re-derives the master list from it.
        launcher.order = picked;

        match (file, note) {
            (Some(p), _) => {
                ui.colored_label(
                    egui::Color32::from_rgb(0x80, 0xc0, 0x80),
                    p.display().to_string(),
                );
            }
            (None, Some(note)) => {
                ui.colored_label(egui::Color32::from_rgb(0xd0, 0xa0, 0x40), note);
            }
            (None, None) => {}
        }
    });
}

/// The masters a plugin's own header names.
pub fn read_masters_for(path: &std::path::Path) -> Vec<String> {
    // Header only: the caller draws this inside a frame, and the whole file is
    // not needed to answer it.
    nvre_esm::Plugin::masters_in_header(path).unwrap_or_default()
}
