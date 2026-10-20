//! Which game a `Data` folder holds, and what that changes.
//!
//! The editor's format work is game-neutral in shape — records are decoded
//! through a schema directory, the load order is a master list, and the asset
//! layer is an archive set over a folder — but every one of those had the game
//! baked into it as a default. This module is where the game becomes a value
//! instead of a constant.
//!
//! A profile names: the folder Steam installs the game into, the game's master,
//! its own masters in load order, the `SArchiveList` it reads its assets from,
//! which archive container those are, and where that game's record schemas
//! live. Everything else — the TES4 container, the GRUP tree, the FormID
//! numbering model — is shared, so a profile is deliberately small.
//!
//! The masters a *session* uses are read from the game's own Data folder
//! ([`GameProfile::masters_in`]), not from the profile: the profile says which
//! order the game's own masters load in and which are its DLC, and only the
//! ones the install actually holds become masters. A profile is therefore
//! correct for a partial install, and picks up a mod master the user added.
//!
//! What a profile does **not** carry, because it is not implemented yet:
//!
//! - the NIF schema, which is still the FNV corpus's
//!   (`NVForever/schemas/nif`, versions 20.0.0.4–20.2.0.7, `bs_version` 11–34),
//!   so a non-FNV game's meshes are decoded with the wrong version's
//!   conditions until a per-game overlay exists;
//! - the record schemas themselves: only `schemas/fnv` is measured, so opening
//!   another game's master fails with `no schema files in ...` rather than
//!   silently misreading it;
//! - the GECK/CK object-window group table, which is FNV's
//!   ([`super::session::geck_group_for_sig`]) until each game has its own;
//! - archive containers other than BSA v104 (Skyrim SE and Fallout 4 ship BA2,
//!   Skyrim SE's BSA is v105/LZ4). Those games open with loose files only and
//!   say so through [`GameProfile::notes`].
//!
//! Every list here is taken from the game's own shipped defaults, cited in the
//! field's doc comment. None of it is guessed.

use std::path::{Path, PathBuf};

/// The archive container a game ships its assets in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    /// BSA v104, zlib: Fallout 3, Fallout: New Vegas, Skyrim (Legendary).
    /// [`fnv_vfs::Vfs`] reads this, so the profile's archive list is used.
    Bsa104,
    /// BSA v105 (LZ4) or BA2: Skyrim SE, Fallout 4. No reader yet.
    Unreadable,
}

/// Everything the editor needs to know about a game before it can open its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameProfile {
    /// Short id used on the command line: `fnv`, `fo3`, `skyrim`, `skyrimse`, `fo4`.
    pub id: &'static str,
    /// The game's name, for titles and the UI.
    pub name: &'static str,
    /// What to call the editor for this game. FNV keeps its own name so nothing
    /// the user already knows changes; the rest are named after the game.
    pub editor_name: &'static str,
    /// The base game's master, and the default plugin a session opens.
    pub master: &'static str,
    /// Base game and DLC masters in the order the game loads them. This is the
    /// default load order, so a session opened over a master sees its own game.
    pub masters: &'static [&'static str],
    /// The `SArchiveList` the shipped INI names, lowest precedence first.
    pub archives: &'static [&'static str],
    /// The container those archives are in.
    pub archive_format: ArchiveFormat,
    /// The folder Steam installs this game into, under `steamapps/common/`.
    /// `None` for a game with no Steam install. The folder name is the game's
    /// own, not a guess at it: `Skyrim Special Edition` installs into that
    /// name rather than `Skyrim`, and the profile says so.
    pub steam_folder: Option<&'static str>,
    /// This game's record schemas, as a subdirectory of the repo's `schemas/`.
    pub schema_subdir: &'static str,
    /// What this game cannot do yet, for the UI to say plainly rather than
    /// showing an empty viewport as if it were the game's fault.
    pub notes: &'static [&'static str],
}

/// Fallout: New Vegas — the editor's own game, and the only one whose record
/// schemas are measured against real data.
pub const FNV: GameProfile = GameProfile {
    id: "fnv",
    name: "Fallout: New Vegas",
    editor_name: "New Vegas Rust Editor",
    master: "FalloutNV.esm",
    masters: &[
        "FalloutNV.esm",
        "DeadMoney.esm",
        "HonestHearts.esm",
        "OldWorldBlues.esm",
        "LonesomeRoad.esm",
        "GunRunnersArsenal.esm",
        "ClassicPack.esm",
        "MercenaryPack.esm",
        "TribalPack.esm",
        "CaravanPack.esm",
    ],
    archives: &[
        "Fallout - Textures.bsa",
        "Fallout - Textures2.bsa",
        "Fallout - Meshes.bsa",
        "Fallout - Voices1.bsa",
        "Fallout - Sound.bsa",
        "Fallout - Misc.bsa",
    ],
    archive_format: ArchiveFormat::Bsa104,
    steam_folder: Some("Fallout New Vegas"),
    schema_subdir: "fnv",
    notes: &[],
};

/// Fallout 3. Same BSA v104 container and the same Gamebryo NIF versions as
/// FNV, so the asset layer already works; only the record layouts are missing.
pub const FO3: GameProfile = GameProfile {
    id: "fo3",
    name: "Fallout 3",
    editor_name: "Fallout 3 Rust Editor",
    master: "Fallout3.esm",
    masters: &[
        "Fallout3.esm",
        "Anchorage.esm",
        "ThePitt.esm",
        "BrokenSteel.esm",
        "PointLookout.esm",
        "Zeta.esm",
    ],
    // The shipped `Fallout_default.ini`'s `SArchiveList` [documented: Bethesda,
    // Fallout 3 `Fallout_default.ini`; the same six names the GECK wiki's BSA
    // page and every archive-invalidation guide quote].
    archives: &[
        "Fallout - Textures.bsa",
        "Fallout - Meshes.bsa",
        "Fallout - Voices.bsa",
        "Fallout - Sound.bsa",
        "Fallout - MenuVoices.bsa",
        "Fallout - Misc.bsa",
    ],
    archive_format: ArchiveFormat::Bsa104,
    steam_folder: Some("Fallout 3"),
    schema_subdir: "fo3",
    notes: &["no record schemas yet: schemas/fo3 has not been measured against Fallout3.esm"],
};

/// Skyrim (Legendary Edition). BSA v104, like FNV and Fallout 3.
pub const SKYRIM: GameProfile = GameProfile {
    id: "skyrim",
    name: "Skyrim",
    editor_name: "Skyrim Rust Editor",
    master: "Skyrim.esm",
    masters: &[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ],
    // The shipped `Skyrim.ini`'s `sResourceArchiveList` [documented: Bethesda,
    // Skyrim `Skyrim.ini`; the same list STEP's INI guide quotes].
    archives: &[
        "Skyrim - Misc.bsa",
        "Skyrim - Shaders.bsa",
        "Skyrim - Textures.bsa",
        "Skyrim - Interface.bsa",
        "Skyrim - Animations.bsa",
        "Skyrim - Meshes.bsa",
        "Skyrim - Sounds.bsa",
    ],
    archive_format: ArchiveFormat::Bsa104,
    steam_folder: Some("Skyrim"),
    schema_subdir: "skyrim",
    notes: &[
        "no record schemas yet: schemas/skyrim has not been measured against Skyrim.esm",
        "NIF schema is the FNV corpus's; Skyrim's NIFs are bs_version 83 and would decode with the wrong conditions",
    ],
};

/// Skyrim Special Edition. Its archives are BA2, and its BSA v105 (where a mod
/// ships one) is LZ4 — neither is readable yet.
pub const SKYRIM_SE: GameProfile = GameProfile {
    id: "skyrimse",
    name: "Skyrim Special Edition",
    editor_name: "Skyrim SE Rust Editor",
    master: "Skyrim.esm",
    masters: &[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ],
    archives: &[],
    archive_format: ArchiveFormat::Unreadable,
    steam_folder: Some("Skyrim Special Edition"),
    schema_subdir: "skyrimse",
    notes: &[
        "no record schemas yet: schemas/skyrimse has not been measured against Skyrim.esm",
        "archives are BA2 and (for mods) BSA v105/LZ4, neither of which is readable yet: loose files only",
        "NIF schema is the FNV corpus's; Skyrim SE's NIFs are bs_version 130+ and are rejected by the header",
    ],
};

/// Fallout 4. BA2 archives, ESL plugins, and a record set the least shared with
/// FNV's — the largest of the four to bring up.
pub const FALLOUT_4: GameProfile = GameProfile {
    id: "fo4",
    name: "Fallout 4",
    editor_name: "Fallout 4 Rust Editor",
    master: "Fallout4.esm",
    masters: &[
        "Fallout4.esm",
        "DLCRobot.esm",
        "DLCworkshop01.esm",
        "DLCCoast.esm",
        "DLCWorkshop02.esm",
        "DLCWorkshop03.esm",
        "DLCNukaWorld.esm",
        "DLCUltraHighResolution.esm",
    ],
    archives: &[],
    archive_format: ArchiveFormat::Unreadable,
    steam_folder: Some("Fallout 4"),
    schema_subdir: "fo4",
    notes: &[
        "no record schemas yet: schemas/fo4 has not been measured against Fallout4.esm",
        "archives are BA2, which is not readable yet: loose files only",
        "ESL plugins (header flag 0x200) and the 0xFE000800 FormID range are not modelled yet",
        "NIF schema is the FNV corpus's; Fallout 4's NIFs are bs_version 130+ and are rejected by the header",
    ],
};

/// Every profile the editor knows, in the order [`detect`] tries them.
pub const ALL: [GameProfile; 5] = [FNV, FO3, SKYRIM, SKYRIM_SE, FALLOUT_4];

impl GameProfile {
    /// The profile whose id is `id`, case-insensitively.
    pub fn by_id(id: &str) -> Option<&'static GameProfile> {
        ALL.iter().find(|g| g.id.eq_ignore_ascii_case(id))
    }

    /// Which game `data_dir` holds, or [`FNV`] when it holds none of them.
    ///
    /// Detection is by the master the game ships, not by an executable name
    /// where the data alone says it: Skyrim LE and SE both ship `Skyrim.esm`,
    /// and only the SE launcher tells them apart, so that pair is checked first.
    pub fn detect(data_dir: &Path) -> &'static GameProfile {
        let has = |name: &str| data_dir.join(name).exists();
        if has("FalloutNV.esm") {
            return &FNV;
        }
        if has("Fallout3.esm") {
            return &FO3;
        }
        if has("Fallout4.esm") {
            return &FALLOUT_4;
        }
        if has("SkyrimSE.exe") || has("SkyrimSE") {
            return &SKYRIM_SE;
        }
        if has("Skyrim.esm") {
            return &SKYRIM;
        }
        &FNV
    }

    /// The game's Data folder: the user's saved override if there is one,
    /// otherwise the Steam install.
    ///
    /// The saved override wins because it is the user's own answer to "where is
    /// this game", and it may name a folder Steam does not know about — a
    /// extracted copy, a second copy for testing. A saved path that no longer
    /// exists is ignored, so an unmounted drive falls back to Steam rather than
    /// opening a folder that is not there.
    pub fn data_dir(&self) -> Option<PathBuf> {
        if let Some(saved) = saved_data_dir(self.id) {
            return Some(saved);
        }
        self.steam_data_dir()
    }

    /// The game's Steam install, if it has one: the `Data` folder under
    /// `steamapps/common/<folder>/`.
    ///
    /// Each profile names the folder Steam installs it into, and this checks
    /// that folder for the game's own master in every Steam library. It is an
    /// existence check, not a discovery: a folder Steam named differently is
    /// simply not found, and the launcher leaves the path empty for the user to
    /// fill in — which is the honest answer rather than a guess that might
    /// point at the wrong install.
    pub fn steam_data_dir(&self) -> Option<PathBuf> {
        let folder = self.steam_folder?;
        for lib in steam_libraries() {
            let data = lib.join("steamapps/common").join(folder).join("Data");
            if data.join(self.master).exists() {
                return Some(data);
            }
        }
        None
    }

    /// The masters present in `data_dir`, in the game's own load order.
    ///
    /// The game's shipped masters come first in the order the game loads them,
    /// then any other `.esm` in the folder in name order — a mod master the
    /// user has installed, which the game loads after its own. `.esp` files are
    /// not masters and are never included.
    ///
    /// Reading the folder rather than a fixed list is what makes this correct
    /// for a partial install: only Fallout 3's DLC the user actually owns
    /// appears, and a folder holding only the base game gives a one-master
    /// list instead of five names the game cannot load.
    pub fn masters_in(&self, data_dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(data_dir) else {
            return Vec::new();
        };
        // Present in the folder, by exact name.
        let present: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                let l = n.to_ascii_lowercase();
                l.ends_with(".esm") || l.ends_with(".esl")
            })
            .collect();
        let has = |name: &str| present.iter().any(|p| p.eq_ignore_ascii_case(name));
        // The game's own order first; a shipped master the install lacks is
        // skipped rather than offered as a master that cannot be loaded.
        let mut out: Vec<String> = Vec::new();
        for m in self.masters {
            if has(m) {
                out.push((*m).to_string());
            }
        }
        // Then everything else the folder holds, in name order.
        let mut extra: Vec<String> = present
            .into_iter()
            .filter(|p| !out.iter().any(|o| o.eq_ignore_ascii_case(p)))
            .collect();
        extra.sort_by_key(|n| n.to_ascii_lowercase());
        out.extend(extra);
        out
    }

    /// Where this game's record schemas live: the repo's `schemas/<game>`.
    ///
    /// A missing directory is reported by `Schema::load_dir` as
    /// `no schema files in ...`, which is the honest answer for a game whose
    /// layouts have not been measured yet.
    pub fn esm_schema_dir(&self) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas")
            .join(self.schema_subdir)
    }

    /// Open the game's assets: its archive set over `data_dir`, plus the NIF
    /// schema.
    ///
    /// The archive order comes from the profile, so each game reads the
    /// `SArchiveList` its own INI names rather than FNV's. A game whose
    /// container is not readable yet opens with loose files only and logs why:
    /// the editor can still edit that game's plugins, it just cannot render
    /// archived meshes.
    pub fn open_asset_data(
        &self,
        data_dir: &Path,
        nif_schema_dir: &Path,
    ) -> Result<nvf_assets::GameData, String> {
        let order = match self.archive_format {
            ArchiveFormat::Bsa104 => {
                fnv_vfs::Vfs::archive_order(data_dir, self.archives, self.masters)
            }
            ArchiveFormat::Unreadable => {
                tracing::warn!(
                    "{}: {} archives are not readable yet; opening with loose files only",
                    self.name,
                    match self.id {
                        "skyrimse" => "BA2 / BSA v105",
                        _ => "BA2",
                    }
                );
                Vec::new()
            }
        };
        let vfs = fnv_vfs::Vfs::open(data_dir, &order).map_err(|e| e.to_string())?;
        let schema = fnv_nif::Schema::load_dir(nif_schema_dir).map_err(|e| e.to_string())?;
        Ok(nvf_assets::GameData {
            vfs: std::sync::Arc::new(vfs),
            schema: std::sync::Arc::new(schema),
        })
    }

    /// The top-level object-window group for a record signature.
    ///
    /// FNV's table is measured against GECK; the other games' editors use
    /// different category names, so until each is written a signature there
    /// falls through to the one catch-all group rather than being filed under
    /// an FNV heading it does not belong in.
    pub fn group_for_sig(&self, sig: &[u8; 4]) -> Option<&'static str> {
        match self.id {
            "fnv" => super::session::geck_group_for_sig(sig),
            _ => None,
        }
    }
}

/// Remember a game's Data folder so the user does not type it again.
///
/// Written to `~/.config/nvre/data-dirs.json` as `{ "<game id>": "<path>" }`.
/// A write failure is silent: the override still applies for this session, and
/// the file is a convenience rather than state the editor needs.
pub fn save_data_dir(game_id: &str, dir: &Path) {
    let path = match config_path() {
        Some(p) => p,
        None => return,
    };
    let mut map = load_data_dirs();
    map.insert(game_id.to_string(), dir.to_string_lossy().into_owned());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(&map).unwrap_or_default();
    let _ = std::fs::write(&path, text);
}

/// The remembered Data folder for `game_id`, if the user set one.
///
/// A remembered path is only used when the folder still exists: a drive that is
/// not mounted is not a Data folder, and falling back to the Steam install or
/// an empty field beats opening a folder that is not there.
pub fn saved_data_dir(game_id: &str) -> Option<PathBuf> {
    let dir = load_data_dirs().remove(game_id)?;
    let path = PathBuf::from(dir);
    path.exists().then_some(path)
}

/// Every remembered Data folder, by game id.
fn load_data_dirs() -> std::collections::BTreeMap<String, String> {
    let Some(path) = config_path() else {
        return Default::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Default::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Where the remembered folders live.
fn config_path() -> Option<PathBuf> {
    // XDG, with the fallback the spec names when it is unset.
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("nvre").join("data-dirs.json"))
}

/// Every Steam library folder, in no particular order.
///
/// Steam writes `libraryfolders.vdf` under its own `steamapps/`, and each entry
/// there carries a `path` — one per library. The default library's own path is
/// the folder Steam itself lives in, and it is always entry `0`. A second
/// library on another drive is another entry, which is why a game is found
/// wherever it was installed rather than only on the system drive.
///
/// The file is read as text and its `path` fields picked out with a scan: it is
/// Valve's KeyValues format, and only this one field is needed.
fn steam_libraries() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let Some(home) = std::env::var_os("HOME") else {
        return out;
    };
    // The documented Linux location. Steam on another platform writes the same
    // file under its own install root; this covers the common case and returns
    // nothing rather than guessing when it is absent.
    let root = PathBuf::from(home).join(".local/share/Steam");
    let vdf = root.join("steamapps/libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(&vdf) {
        out.push(root.clone());
        for path in vdf_paths(&text) {
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

/// Every `path` value in a `libraryfolders.vdf`.
fn vdf_paths(text: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("\"path\"") {
            continue;
        }
        if let Some(v) = vdf_value(line) {
            out.push(PathBuf::from(v));
        }
    }
    out
}

/// The value of a VDF `"key" "value"` line: the text inside the second pair of
/// quotes.
fn vdf_value(line: &str) -> Option<String> {
    let mut parts = line.split('"').skip(2).step_by(2);
    parts.next().map(|s| s.to_string())
}
