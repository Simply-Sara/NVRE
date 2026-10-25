//! What order the launcher's plugin lists are in, and what order a session's
//! masters load in.
//!
//! Three answers, and they are not the same question:
//!
//! - **file name**: the folder sorted by name. It is what a plain file list is,
//!   and the only order a folder with no order file has.
//! - **`plugins.txt`**: the plugins the game has active, in the order it loads
//!   them.
//! - **`loadorder.txt`**: every plugin, active or not, in load order.
//!
//! Which file exists — and whether it means anything — is the game's business,
//! and this module does not decide it: it reads whichever the user picked, and
//! says when it is not there.
//!
//! For Fallout 3, Fallout: New Vegas and Oblivion the engine orders plugins by
//! **file timestamp** and `plugins.txt` only records which are active, so the
//! file is a record rather than the mechanism [documented: LOOT, "Introduction
//! to Load Orders" — "load order is determined by plugin timestamps, and the
//! plugins you have accessed are listed in `%LOCALAPPDATA%\plugins.txt`"]. A
//! mod manager's `loadorder.txt` is the order a tool believes in, and neither
//! file is what the engine will do. From Skyrim 1.4.26 on, and in Fallout 4,
//! order is the file's own line order, `plugins.txt` is authoritative, and
//! `loadorder.txt` exists so tools can carry an order for the plugins that are
//! *not* active.
//!
//! A plugin the file names comes first, in the file's order; anything it does
//! not name follows in file-name order. A Data folder holds plugins the file has
//! never heard of — an inactive `.esp`, a master installed since the game last
//! ran — and dropping them would hide them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Which order a plugin list is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PluginOrder {
    /// By file name: the folder as a file list.
    ///
    /// A **master** list set to this keeps the order the masters were found in
    /// rather than its name order: a master list is a load order, and
    /// `FalloutNV.esm` sorted after the plugins that require it is a list the
    /// game cannot load. See `Launcher::resort_masters`.
    #[default]
    Name,
    /// The order the game's own `plugins.txt` names.
    PluginsTxt,
    /// The order a mod manager's `loadorder.txt` names.
    LoadOrderTxt,
}

impl PluginOrder {
    /// Every order the launcher offers, in the order the drop-down lists them.
    pub const ALL: [PluginOrder; 3] = [
        PluginOrder::Name,
        PluginOrder::PluginsTxt,
        PluginOrder::LoadOrderTxt,
    ];

    /// The file this order is read from. `None` for [`PluginOrder::Name`],
    /// which is not read from anywhere.
    pub fn file(self) -> Option<&'static str> {
        match self {
            PluginOrder::Name => None,
            PluginOrder::PluginsTxt => Some("plugins.txt"),
            PluginOrder::LoadOrderTxt => Some("loadorder.txt"),
        }
    }

    /// What the drop-down shows for this order.
    pub fn label(self) -> &'static str {
        match self {
            PluginOrder::Name => "File name (A-Z)",
            PluginOrder::PluginsTxt => "plugins.txt",
            PluginOrder::LoadOrderTxt => "loadorder.txt",
        }
    }
}

/// An order file that was read.
#[derive(Debug, Clone)]
pub struct OrderFile {
    /// The file it was read from.
    pub path: PathBuf,
    /// The plugin names it lists, in its order.
    pub names: Vec<String>,
}

/// The plugin names `order` names, in its own order, read from `dir`.
///
/// `None` when the folder has no such file. That is the honest answer for a
/// folder the game keeps no order file in, and for a game that has none at all:
/// the caller reports it rather than showing a list that looks ordered by a file
/// that is not there.
///
/// `dir` alone is not enough on Windows, where the game keeps these files under
/// `%LOCALAPPDATA%` rather than in `Data`; [`read_for_game`] is the entry point
/// that searches both.
pub fn read(dir: &Path, order: PluginOrder) -> Option<OrderFile> {
    let file = order.file()?;
    read_file(&dir.join(file))
}

/// Read one order file by its own path.
///
/// The file the user picked in the launcher, which may be anywhere at all — a
/// mod manager's profile, a backup, a list they keep by hand. The rules are the
/// same ones [`read`] applies, because the format does not know where it came
/// from.
pub fn read_file(path: &Path) -> Option<OrderFile> {
    let bytes = std::fs::read(path).ok()?;
    Some(OrderFile {
        path: path.to_path_buf(),
        names: parse(&decode(&bytes)),
    })
}

/// Where an order file may live, most authoritative first.
///
/// A plugin lives in the Data folder, but the order files are the game's *user*
/// state and on Windows live in `%LOCALAPPDATA%\<game>\` beside the INIs and
/// saves — not in `Data` at all [documented: LOOT's "plugins you have accessed
/// are listed in `%LOCALAPPDATA%\plugins.txt`"]. So both are searched:
///
/// 1. **LocalAppData**, which is where the game and every mod manager put the
///    real file on Windows. It comes first because a copy in `Data` is not where
///    the game reads from, so the user's actual order must win when both exist.
/// 2. **The Data folder**, which is the only place a portable install or a
///    hand-placed copy has it, and is the whole answer on Linux, where there is
///    no LocalAppData convention for these games at all.
///
/// A Proton prefix is a Windows layout on a Linux disk, so `%LOCALAPPDATA%`
/// there is `…/compatdata/<appid>/pfx/drive_c/users/<user>/AppData/Local`. That
/// is found through the same environment variable when the editor itself runs
/// under Proton; when it does not, the Data-folder entry is what applies and the
/// prefix is not guessed at.
pub fn order_dirs(game: &crate::game::GameProfile, data_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(folder) = game.local_appdata_folder {
        if let Some(local) = local_appdata() {
            dirs.push(local.join(folder));
        }
    }
    dirs.push(data_dir.to_path_buf());
    dirs
}

/// The user's LocalAppData folder, as the OS names it.
///
/// `%LOCALAPPDATA%` on Windows, and the same variable inside a Proton prefix
/// (which is why one rule covers both). On a plain Linux session there is no
/// such folder for these games, so this is `None` and the Data folder is the
/// only place searched — which is correct rather than a fallback.
fn local_appdata() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

/// Read `order` for `game`, searching every place it may live.
///
/// Returns the first file found, and the path says which location that was, so
/// the launcher can show it rather than implying the Data folder.
pub fn read_for_game(
    game: &crate::game::GameProfile,
    data_dir: &Path,
    order: PluginOrder,
) -> Option<OrderFile> {
    let file = order.file()?;
    for dir in order_dirs(game, data_dir) {
        if let Some(found) = read_file(&dir.join(file)) {
            return Some(found);
        }
    }
    None
}

/// The file names in an order file's text, in order.
///
/// One name per line. Blank lines and `#` comments are skipped, and a leading
/// `*` is dropped — the games from Skyrim Special Edition on mark an active
/// plugin that way. Every other line is a file name as written, including one
/// that matches nothing in the folder: the caller's job is to order what it has,
/// and the file's job is not to be a listing of it.
fn parse(text: &str) -> Vec<String> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(|l| l.strip_prefix('*').unwrap_or(l).trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Decode an order file's bytes.
///
/// `plugins.txt` is Windows-1252 and `loadorder.txt` is UTF-8 [documented: the
/// two encodings every mod manager's documentation states], but the two agree on
/// every byte a plugin file name uses, so one rule reads both: UTF-8 when it
/// decodes, Windows-1252 when it does not. A CP1252 byte above 0x7F is not valid
/// UTF-8 on its own, so the two can only be confused on bytes that are the same
/// either way.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => nvre_util::cp1252::decode(bytes),
    }
}

/// Where each plugin sits in an order file.
#[derive(Debug, Clone, Default)]
pub struct Rank(HashMap<String, usize>);

impl Rank {
    /// Whether the rank names nothing, so ordering by it changes nothing.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Build a rank from an order file's names, as [`read`] returns them.
    ///
    /// Names are matched case-insensitively, the way the engine and the load
    /// order match them, so a file that spells a name differently from the disk
    /// still orders the plugin it means.
    pub fn of(names: &[String]) -> Rank {
        let mut map: HashMap<String, usize> = HashMap::with_capacity(names.len());
        for (i, name) in names.iter().enumerate() {
            // The first line wins: a file that names a plugin twice is not a
            // load order, and the game reads the earlier line.
            map.entry(name.to_ascii_lowercase()).or_insert(i);
        }
        Rank(map)
    }

    /// Where the file puts `name`, or `None` when it does not name it.
    pub fn position(&self, name: &str) -> Option<usize> {
        self.0.get(&name.to_ascii_lowercase()).copied()
    }
}

/// Where a plugin the file does not name goes: after every plugin it does.
///
/// The key's second half then decides, and that is the file name — the same
/// order the list itself is in, so the plugins a file does not mention read the
/// way they would with no order file at all.
pub const UNRANKED: usize = usize::MAX;

/// Sort `paths` into `rank`'s order, keeping file-name order for the rest.
pub fn order_paths(paths: &mut [PathBuf], rank: &Rank) {
    paths.sort_by_key(|p| {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        (rank.position(&name).unwrap_or(UNRANKED), name)
    });
}
