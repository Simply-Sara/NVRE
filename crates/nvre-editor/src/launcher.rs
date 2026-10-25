//! The launcher: pick the plugin to edit, create a new plugin, and configure masters.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::plugin_order::{self, PluginOrder};

/// The launcher's primary action: Open, or Create & Open.
///
/// Right-aligned in the bottom bar by the caller, which is why it is a function
/// of its own rather than an inline match: the row it lives in also carries the
/// selected file's dates, and mixing the two inline made the row unreadable.
fn action_button(
    ui: &mut egui::Ui,
    launcher: &Launcher,
    open: &mut MessageWriter<crate::OpenRequested>,
) {
    match launcher.tab {
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
            // A name is all a new plugin needs. Zero masters is a plugin GECK
            // writes too — `fonv_geck_empty_no_master.esp` is a TES4 header and
            // nothing else — so an empty master list is a choice, not an
            // unfinished form.
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
    }
}

/// A file's created and modified times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileTimes {
    /// When the file was created, if the filesystem records it.
    pub created: Option<std::time::SystemTime>,
    /// When it was last modified.
    pub modified: Option<std::time::SystemTime>,
}

impl FileTimes {
    /// Read a file's times, or `None` when it cannot be stat'ed.
    pub fn of(path: &std::path::Path) -> Option<FileTimes> {
        let meta = std::fs::metadata(path).ok()?;
        Some(FileTimes {
            created: meta.created().ok(),
            modified: meta.modified().ok(),
        })
    }
}

/// A timestamp as `2026-10-25 16:31`, in the local time zone.
///
/// The `time` crate with the local-offset feature, because a file's time is
/// something a person reads in their own zone and `SystemTime` alone is a
/// duration since 1970. Formatted to the minute: a plugin's mtime to the second
/// is noise, and the format is what a file browser shows.
fn format_time(t: std::time::SystemTime) -> String {
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let dt = time::OffsetDateTime::from(t).to_offset(offset);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        dt.year(),
        u8::from(dt.month()),
        dt.day(),
        dt.hour(),
        dt.minute()
    )
}

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
    /// The selected file's created and modified times, as the OS reports them.
    ///
    /// Read once when the file is picked rather than every frame: a `stat` per
    /// frame is a syscall per frame, and the values cannot change while the
    /// launcher is up unless something else writes the file — at which point a
    /// restart is the honest answer anyway.
    ///
    /// `created` is `None` on a filesystem that does not record it, which is
    /// most of them on Linux: the format has no birth time, so a `.esp` on ext4
    /// often has none.
    pub active_times: Option<FileTimes>,
    /// The selected file's own header: its masters, author and description,
    /// read from its `TES4` record without parsing the plugin.
    ///
    /// The masters the file declares *are* the master list for this tab: a
    /// plugin is opened with the masters its header names and nothing else, so
    /// there is nothing to tick. This is what the right-hand column shows.
    pub header: nvre_esm::HeaderSummary,
    /// The masters the session will open with, in load order.
    ///
    /// On **Create New** this is the set the user ticks, which is why it is a
    /// list of its own rather than the header's: the plugin does not exist yet.
    /// On **Open Existing** it is derived — the selected file's own `MAST`
    /// names, put in the order the chosen order file (or the headers) gives.
    ///
    /// The Create New default is the game's own base master alone
    /// ([`Launcher::default_masters`]); the rest of the folder is offered as
    /// tickable rows.
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
    /// What the last attempt to load said.
    pub error: Option<String>,
    /// The plugins found in the Data folder, and the folder they came from.
    pub listing: Vec<std::path::PathBuf>,
    /// What `listing` was built from: the folder, and the order it was read
    /// with. The launcher draws every frame, so the folder is listed and the
    /// order applied only when one of these changes.
    pub listing_key: Option<(std::path::PathBuf, PluginOrder, Option<std::path::PathBuf>)>,
    /// The order file the current order was read from, when it was found.
    pub order_file: Option<std::path::PathBuf>,
    /// An order file the user picked by hand, which replaces the search.
    ///
    /// `Some` while a picked file is in force: the drop-down still says which
    /// *kind* of order this is, but the file comes from wherever the picker
    /// pointed — a mod manager's profile, a backup, a list kept by hand. The
    /// launcher's search is skipped entirely, because a picked file that was
    /// silently overridden by a discovered one would be the opposite of what
    /// picking it meant.
    ///
    /// Cleared when the user picks a different order from the drop-down: that
    /// is a request for the game's own file, not for the last picked one.
    pub custom_order: Option<std::path::PathBuf>,
    /// Why a picked order file could not be read, for the launcher to say.
    ///
    /// Separate from `order_note` so a failed pick reads as a failure rather
    /// than as "the game has no such file".
    pub custom_error: Option<String>,
    /// Why the order file could not be used, for the launcher to say.
    ///
    /// An order the user asked for and did not get has to say so: a list in
    /// file-name order looks exactly like an order file that was read and
    /// happened to agree.
    pub order_note: Option<String>,
}

impl Launcher {
    /// A fresh launcher for `game`, on the **Open Existing** tab.
    ///
    /// The launcher opens on that tab, so the master list is what *it* needs: the
    /// selected file's own `MAST` names once a file is chosen, and the folder's
    /// masters until then. `for_game` is called once at startup, which is when
    /// the game is known.
    ///
    /// The **Create New** tab's default is different and is [`default_masters`]:
    /// the base master alone, because a new plugin depends on the base game and
    /// nothing else until it says so.
    pub fn for_game(game: &crate::game::GameProfile) -> Launcher {
        let dir = game.data_dir().unwrap_or_default();
        Launcher {
            tab: LauncherTab::OpenExisting,
            active: None,
            active_times: None,
            header: Default::default(),
            // The base master alone, not every installed ESM. See
            // `default_masters`.
            masters: default_masters(game, &dir),
            order: PluginOrder::Name,
            new_name: "NewPlugin.esp".to_string(),
            new_is_master: false,
            // Empty, so the header gets GECK's `DEFAULT` author and no
            // description unless the user types one.
            new_author: String::new(),
            new_description: String::new(),
            error: None,
            listing: Vec::new(),
            listing_key: None,
            order_file: None,
            custom_order: None,
            custom_error: None,
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
        self.active_times = None;
        self.header = Default::default();
        self.order_file = None;
        self.order_note = None;
        // A file picked for one game is not an order for another: the pick is
        // cleared rather than carried across, so a switch cannot leave the
        // previous game's list in force under the new game's name.
        self.custom_order = None;
        self.custom_error = None;
        let Some(dir) = dir else {
            self.masters.clear();
            return;
        };
        self.refresh_listing(game, Some(dir));
    }

    /// Switch between the two tabs, re-deriving the master list for the one
    /// being entered.
    ///
    /// The two tabs' master lists mean different things — a file's own `MAST`
    /// names on Open Existing, the game's base master on Create New — so the
    /// list cannot be carried across a tab switch. Without this, the Create New
    /// tab inherited whatever the Open Existing tab had derived, which is why
    /// the DLC were all still ticked until the game was switched away and back.
    ///
    pub fn set_tab(
        &mut self,
        tab: LauncherTab,
        game: &crate::game::GameProfile,
        dir: Option<&std::path::Path>,
    ) {
        self.tab = tab;
        // The listing itself is not re-read — the folder has not changed — but
        // the listing key must move, so the next `refresh_listing` re-reads and
        // re-derives the master list for the tab now in front. Leaving the key
        // alone made the next refresh short-circuit and the old list survive.
        self.listing_key = None;
        self.refresh_listing(game, dir);
    }

    /// Point the master list at `path`, the file just selected.
    ///
    /// On this tab the master list is **derived, not chosen**: a plugin is
    /// opened with the masters its own `MAST` header names and nothing else, so
    /// the list is those names put in load order. It is derived through the same
    /// `masters_in_with` the order file feeds, which keeps the headers a
    /// constraint — a master the file names still cannot end up after something
    /// that requires it.
    ///
    /// A name the header declares that the folder does not hold stays in the
    /// list: it is what the file says, and hiding it would make the plugin look
    /// openable when it is not. The column marks it.
    pub fn select_file(&mut self, path: &std::path::Path) {
        let dir = path.parent().unwrap_or(std::path::Path::new("."));
        let rank = self.rank();
        self.masters = crate::game::order_named(&self.header.masters, dir, &rank);
    }

    /// The order file's ranking, for putting a derived list in load order.
    fn rank(&self) -> plugin_order::Rank {
        // `order_file` is only `Some` for a file that was read, so this needs no
        // separate check on the listing.
        self.order_file
            .as_deref()
            .and_then(plugin_order::read_file)
            .map(|f| plugin_order::Rank::of(&f.names))
            .unwrap_or_default()
    }

    /// Re-list the folder and re-read the order, when either has changed.
    ///
    /// The launcher draws every frame, so this is called every frame and does
    /// the work only when the key moves.
    pub fn refresh_listing(
        &mut self,
        game: &crate::game::GameProfile,
        dir: Option<&std::path::Path>,
    ) {
        // What the *file list* depends on: the folder, and the order it is read
        // with. A picked file is part of that key too — switching to it or away
        // from it has to re-read, exactly as changing the drop-down does.
        //
        // The tab is deliberately NOT part of it. The master list belongs to the
        // tab, and `refresh_listing` runs every frame: keying it on the tab would
        // re-derive the master list on every frame and drop the ticks the user had
        // just made. A tab switch re-derives through `set_tab` instead.
        let Some(dir) = dir else {
            self.listing_key = None;
            self.listing.clear();
            self.masters.clear();
            return;
        };
        let key = (dir.to_path_buf(), self.order, self.custom_order.clone());
        if self.listing_key.as_ref() == Some(&key) {
            return;
        }
        self.listing_key = Some(key);
        self.listing = list_plugins(dir);

        // A file the user picked replaces the search rather than joining it: a
        // picked file that a discovered one could override would be the
        // opposite of what picking it meant.
        let found = match &self.custom_order {
            Some(path) => match plugin_order::read_file(path) {
                Some(f) => {
                    self.custom_error = None;
                    Some(f)
                }
                None => {
                    self.custom_error = Some(format!("cannot read {}", path.display()));
                    None
                }
            },
            // Otherwise the game's own file: its user state on Windows, or the
            // Data folder where a portable install or a hand-placed copy has
            // it. The path shown is the one that answered.
            None => plugin_order::read_for_game(game, dir, self.order),
        };

        let Some(file) = found else {
            // No order file: the list is the folder, and the master list is the
            // one the headers give.
            self.order_file = None;
            self.order_note = if self.custom_order.is_some() {
                // The picked file failed, and `custom_error` says why; the
                // search's own answer would only confuse the message.
                None
            } else {
                self.order.file().map(|f| {
                    let dirs = plugin_order::order_dirs(game, dir);
                    let places: Vec<String> =
                        dirs.iter().map(|d| d.display().to_string()).collect();
                    format!("no {f} in {}", places.join(" or "))
                })
            };
            self.derive_masters(game, dir, &plugin_order::Rank::default());
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
        self.derive_masters(game, dir, &rank);
    }

    /// Put the master list in the state this tab wants.
    ///
    /// Only called from the branches of `refresh_listing` that actually re-list
    /// the folder, never on an unchanged frame: re-deriving there would replace
    /// the ticks the Create New tab's user has made with the folder's defaults.
    fn derive_masters(
        &mut self,
        game: &crate::game::GameProfile,
        dir: &std::path::Path,
        rank: &plugin_order::Rank,
    ) {
        self.masters = self.masters_for(game, dir, rank);
    }

    /// The master list this launcher's tab wants.
    ///
    /// Split out because `refresh_listing` is shared: it runs on both tabs, and
    /// the two want different things. **Open Existing** derives the list from
    /// the selected file's `MAST` names; **Create New** starts from the game's
    /// own base master and lets the user tick the rest.
    ///
    /// This is only called when the listing is being re-derived — see
    /// [`Launcher::refresh_listing`]. It is not called on a frame where the
    /// listing is unchanged, because that is how a tick survives.
    fn masters_for(
        &self,
        game: &crate::game::GameProfile,
        dir: &std::path::Path,
        rank: &plugin_order::Rank,
    ) -> Vec<String> {
        if self.tab == LauncherTab::CreateNew {
            default_masters(game, dir)
        } else {
            game.masters_in_with(dir, rank)
        }
    }
}

/// The master a new plugin depends on by default: the game's own base master,
/// when the folder holds it.
///
/// Not every installed ESM. A new plugin depends on the base game and nothing
/// else until it says otherwise — the DLC are optional content, and a plugin
/// written against them will not load without them installed. Pre-ticking all
/// of them made the default plugin depend on five files the user may not own.
///
/// The base master is `GameProfile::master`, and it is checked against the
/// folder rather than trusted: a portable install may lack it, and a
/// master the plugin cannot open is a worse default than none. When the
/// folder does not hold it the list is empty, which the Create New tab's
/// `action_button` already treats as a valid choice — GECK writes plugins with
/// no `MAST` at all.
pub fn default_masters(game: &crate::game::GameProfile, dir: &std::path::Path) -> Vec<String> {
    let held = dir.join(game.master);
    held.is_file()
        .then(|| game.master.to_string())
        .into_iter()
        .collect()
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
    /// Whether the Data-folder row is showing.
    ///
    /// The row is normally folded away — the game's install is found for it, so
    /// the field is a detail rather than a question. The gear beside the game
    /// dropdown unfolds it, and it starts unfolded whenever the folder is
    /// missing or wrong, because then it is the thing the user has to fix.
    pub show_folder_row: bool,
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
        // Unfolded when the folder is missing or does not hold the game's own
        // master: then the field is the thing the user has to fix, and hiding it
        // would leave a launcher that cannot open anything with no visible way
        // out. Folded otherwise, because a found install is not a question.
        let show_folder_row = !data_dir
            .as_ref()
            .is_some_and(|d| d.join(game.master).exists());
        GameChoice {
            game,
            data_dir,
            folder_edit,
            show_folder_row,
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
        // The new game's folder decides whether the row is worth showing: a
        // game with no install found has to show the field it needs filled in.
        self.show_folder_row = !self
            .data_dir
            .as_ref()
            .is_some_and(|d| d.join(game.master).exists());
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
    mut details: ResMut<crate::file_details_window::FileDetailsState>,
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
        launcher.refresh_listing(choice.game, Some(dir));
    }

    let listing = launcher.listing.clone();
    let order_note = launcher.order_note.clone();
    let order_file = launcher.order_file.clone();
    let order_error = launcher.custom_error.clone();

    egui::CentralPanel::default().show(&mut crate::panels::root_ui(ctx, "launcher"), |ui| {
        // Lay out bottom bar anchored to the bottom of the window
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(14.0);

            // Action button anchored at the bottom
            ui.horizontal(|ui| {
                // Details at the bottom left, where GECK's Data Files dialog has
                // it. Only meaningful once a file is picked: there is nothing to
                // list otherwise.
                if launcher.tab == LauncherTab::OpenExisting {
                    ui.add_enabled_ui(launcher.active.is_some(), |ui| {
                        // The same height as the action button it sits opposite
                        // — 32, which is what `action_button` asks for — but
                        // narrower: "Details…" is a shorter label than "Open
                        // FalloutNV.esm", and matching its width would leave a
                        // button mostly empty. Equal heights, sized to content.
                        if ui
                            .add_sized([110.0, 32.0], egui::Button::new("Details…"))
                            .on_hover_text("Every record in the selected file")
                            .clicked()
                        {
                            if let Some(path) = &launcher.active {
                                details.open(path.clone());
                            }
                        }
                    });
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // The action button, rightmost.
                    action_button(ui, &launcher, &mut open);
                    // The selected file's times, immediately left of it and
                    // stacked two lines.
                    //
                    // They live in this row rather than in one of their own
                    // because the bottom bar is laid out `bottom_up` inside
                    // the panel: a row of its own would take its height from
                    // the body above, shortening the file list for a label
                    // that only occupies the right-hand side.
                    if let Some(times) = launcher.active_times {
                        let created = times
                            .created
                            .map(format_time)
                            .unwrap_or_else(|| "(not recorded)".to_string());
                        let modified = times
                            .modified
                            .map(format_time)
                            .unwrap_or_else(|| "(unknown)".to_string());
                        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                            ui.weak(format!("created  {created}"));
                            ui.weak(format!("modified {modified}"));
                        });
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
                    // The gear unfolds the Data-folder row. A button rather than
                    // a bare glyph, so it reads as clickable, and it highlights
                    // while the row is showing so the state is visible.
                    let open = choice.show_folder_row;
                    if ui
                        .selectable_label(open, "⚙")
                        .on_hover_text("Show or hide the Data folder")
                        .clicked()
                    {
                        choice.show_folder_row = !open;
                    }
                });

                // The Data folder, folded away unless the gear is pressed or the
                // folder is missing — see `GameChoice::show_folder_row`. Filled
                // from Steam when the install is there; empty for the user to
                // fill when it is not. The buffer lives in the resource so typing
                // is not discarded every frame.
                if choice.show_folder_row {
                    ui.horizontal(|ui| {
                        ui.label("Data folder:");
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut choice.folder_edit)
                                .desired_width(360.0)
                                .hint_text("/path/to/<Game>/Data"),
                        );
                        // The OS's own folder picker, to the right of the field.
                        // A chosen folder is committed on the spot — the picker
                        // is an explicit answer, not a keystroke still being
                        // typed — so the plugin and master lists follow it
                        // immediately rather than waiting for the field to lose
                        // focus.
                        if ui.button("Browse…").clicked() {
                            let mut dialog = rfd::FileDialog::new().set_title(format!(
                                "{} — choose the Data folder",
                                choice.game.name
                            ));
                            // Start where the field points, when it points
                            // somewhere real: the common case is browsing from
                            // the folder already in use.
                            if let Some(dir) = choice.active_dir().filter(|d| d.is_dir()) {
                                dialog = dialog.set_directory(dir);
                            }
                            if let Some(picked) = dialog.pick_folder() {
                                let before = choice.data_dir.clone();
                                choice.pick_folder(&picked);
                                if choice.data_dir != before {
                                    launcher.retarget(choice.game, choice.active_dir().as_deref());
                                }
                                // A folder that now holds the game's master has
                                // answered the question the row was showing.
                                choice.show_folder_row = !picked.join(choice.game.master).exists();
                            }
                        }
                        // Commit on losing focus or Enter: the plugin list and
                        // the master list both follow the folder.
                        if resp.lost_focus() {
                            let before = choice.data_dir.clone();
                            choice.commit_folder();
                            if choice.data_dir != before {
                                launcher.retarget(choice.game, choice.active_dir().as_deref());
                            }
                        }
                    });
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
                        // Through `set_tab`, not a bare field write: the master
                        // list means different things on the two tabs, and a
                        // switch that left it alone carried the other tab's
                        // masters into this one.
                        launcher.set_tab(
                            LauncherTab::OpenExisting,
                            choice.game,
                            active_dir.as_deref(),
                        );
                    }
                    if ui
                        .selectable_label(
                            launcher.tab == LauncherTab::CreateNew,
                            "Create New Plugin (.esp / .esm)",
                        )
                        .clicked()
                    {
                        launcher.set_tab(
                            LauncherTab::CreateNew,
                            choice.game,
                            active_dir.as_deref(),
                        );
                    }
                });
                ui.separator();

                match launcher.tab {
                    LauncherTab::OpenExisting => {
                        // Above the master list it orders, which is the column
                        // on the right.
                        order_picker(
                            ui,
                            &mut launcher,
                            order_file.as_deref(),
                            order_note.as_deref(),
                            order_error.as_deref(),
                        );
                        ui.add_space(4.0);

                        // No `- 40`: the height is measured where the scroll
                        // area is drawn, after the label above it and after the
                        // columns have taken their share. Subtracting a constant
                        // here was a guess at the height of things that had not
                        // been laid out yet, and it left a gap above the bottom
                        // bar.
                        ui.columns(2, |cols| {
                            // Files on left
                            cols[0].label("Files in Data:");
                            // Everything left in the column, measured now.
                            let list_height = cols[0].available_height().max(120.0);
                            egui::ScrollArea::vertical()
                                .id_salt("files_open")
                                .max_height(list_height)
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
                                            launcher.active_times = FileTimes::of(p);
                                            launcher.error = None;
                                            // The header is what the right-hand
                                            // column shows, and the masters are
                                            // the ones it declares — read in one
                                            // pass, since both come from the same
                                            // `TES4` record.
                                            launcher.header = nvre_esm::Plugin::header_summary(p)
                                                .unwrap_or_default();
                                            launcher.select_file(p);
                                        }
                                    }
                                });

                            // The selected file's header, and the masters it
                            // declares. No checkboxes: a plugin is opened with
                            // the masters its own header names, so there is
                            // nothing here to choose.
                            //
                            // The author and description come first because they
                            // are how a user tells one file from another in the
                            // list on the left — GECK's own Data Files dialog
                            // shows the same two.
                            cols[1].group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Author:");
                                    match &launcher.header.author {
                                        Some(a) => ui.label(a),
                                        // GECK writes `CNAM DEFAULT` for a plugin
                                        // nobody has claimed, so an absent author
                                        // is the normal case rather than an
                                        // error.
                                        None => ui.weak("(none)"),
                                    };
                                });
                                // `horizontal_top`, not `horizontal`: with a
                                // three-row box beside it, a vertically centred
                                // label floats in the middle of the field rather
                                // than naming it.
                                ui.horizontal_top(|ui| {
                                    ui.label("Description:");
                                    match &launcher.header.description {
                                        Some(d) => ui.label(d),
                                        None => ui.weak("(none)"),
                                    };
                                });
                            });
                            cols[1].add_space(6.0);
                            cols[1].label("Masters:");
                            if launcher.active.is_some() {
                                // Measured after the author/description group
                                // and the "Masters:" label above it, so the list
                                // reaches the bottom of the column.
                                let masters_height = cols[1].available_height().max(120.0);
                                egui::ScrollArea::vertical()
                                    .id_salt("masters_open")
                                    .max_height(masters_height)
                                    .show(&mut cols[1], |ui| {
                                        if launcher.masters.is_empty() {
                                            ui.label("(this plugin declares no masters)");
                                        }
                                        for name in &launcher.masters {
                                            // Whether the file is installed is
                                            // worth saying: a header naming a
                                            // master the folder lacks cannot be
                                            // opened, and the open itself would
                                            // only report it as a failure.
                                            let present = listing.iter().any(|p| {
                                                p.file_name().is_some_and(|n| {
                                                    n.to_string_lossy().eq_ignore_ascii_case(name)
                                                })
                                            });
                                            if present {
                                                ui.monospace(name);
                                            } else {
                                                ui.colored_label(
                                                    egui::Color32::from_rgb(0xd0, 0xa0, 0x40),
                                                    format!("{name}  (not installed)"),
                                                );
                                            }
                                        }
                                    });
                            } else {
                                cols[1].label("Select a file to see its masters.");
                            }
                        });
                    }
                    LauncherTab::CreateNew => {
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
                        // Below the file name and above the master list, because
                        // it is the master list it orders.
                        order_picker(
                            ui,
                            &mut launcher,
                            order_file.as_deref(),
                            order_note.as_deref(),
                            order_error.as_deref(),
                        );

                        ui.add_space(6.0);
                        // Two columns: the masters on the left, where the list is
                        // long, and the author and description on the right, where
                        // two text fields fit without pushing the list down.
                        ui.columns(2, |cols| {
                            cols[0].label("Select Masters to depend on (Checkboxes):");

                            // Measured where it is drawn, so it reaches the
                            // bottom bar instead of stopping short of it.
                            let masters_height = cols[0].available_height().max(120.0);
                            egui::ScrollArea::vertical()
                                .id_salt("masters_new")
                                .max_height(masters_height)
                                .show(&mut cols[0], |ui| {
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

                            // The author and description the header carries.
                            // GECK's own Create dialog has no such fields — its
                            // plugin is always CNAM `DEFAULT` with no SNAM — so
                            // these are what let a mod say who wrote it and what
                            // it is.
                            // Label above field, not beside it: the fields take
                            // the column's full width that way, which is what the
                            // long description wants, and the two labels line up
                            // with each other rather than with the middle of a
                            // six-row box.
                            cols[1].group(|ui| {
                                ui.label("Author:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut launcher.new_author)
                                        .hint_text("DEFAULT")
                                        .desired_width(f32::INFINITY),
                                );
                                ui.add_space(6.0);
                                ui.label("Description:");
                                // A multi-line field: `SNAM` is a description, and
                                // a single line of it is a caption rather than a
                                // description. Six rows is room to write a
                                // paragraph in.
                                //
                                // It is not a `singleline` with a bigger box:
                                // `TextEdit::multiline` is a different widget, and
                                // a newline typed into a singleline field is
                                // silently dropped.
                                ui.add(
                                    egui::TextEdit::multiline(&mut launcher.new_description)
                                        .hint_text("what this plugin is for")
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(6),
                                );
                            });
                        });
                    }
                }
            });
        });
    });
}

/// The master-list order drop-down, the button that picks a file by hand, and
/// what either did.
///
/// Drawn on both tabs, directly above the master list it orders: the order file
/// is read from the Data folder and its order is the one the game loads masters
/// in, so this is also the order a session's load order is built from — the
/// plugin whose definition wins.
///
/// `file` is the order file that was read and `note` why none was. An order the
/// user asked for and did not get has to say so: a list in file-name order looks
/// exactly like a file that was read and agreed.
///
/// The `Browse…` button appears only for an order that *comes from a file* —
/// `plugins.txt` and `loadorder.txt`, not `File name (A-Z)`, which is read from
/// nowhere — and it opens the OS's own file picker for any `.txt`.
fn order_picker(
    ui: &mut egui::Ui,
    launcher: &mut Launcher,
    file: Option<&std::path::Path>,
    note: Option<&str>,
    error: Option<&str>,
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
        if picked != launcher.order {
            launcher.order = picked;
            // A different order means the game's own file, not the last file
            // picked by hand: keeping the pick would make the drop-down look
            // like it chose a source it is not reading.
            launcher.custom_order = None;
            launcher.custom_error = None;
        }

        // Any `.txt`, because an order file is a list of plugin names and the
        // format is the same wherever it was written — a mod manager's profile,
        // a backup, a list kept by hand.
        if launcher.order.file().is_some() && ui.button("Browse…").clicked() {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Choose an order file")
                .add_filter("Order file", &["txt"]);
            // Start where the file in force is, or in the Data folder, so the
            // picker opens somewhere meaningful rather than at home.
            if let Some(dir) = file.and_then(|p| p.parent()).filter(|d| d.is_dir()) {
                dialog = dialog.set_directory(dir);
            }
            if let Some(chosen) = dialog.pick_file() {
                launcher.custom_order = Some(chosen);
                launcher.custom_error = None;
            }
        }

        // Which file is in force, or why none is. The path says it either way:
        // a picked file is a different answer from a discovered one, and the
        // user has to be able to tell which they are looking at.
        if let Some(err) = error {
            ui.colored_label(egui::Color32::RED, err);
        } else {
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
        }
    });
}
