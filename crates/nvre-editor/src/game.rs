//! Which game a `Data` folder holds, and what that changes.
//!
//! The editor's format work is game-neutral in shape — records are decoded
//! through a schema directory, the load order is a master list, and the asset
//! layer is an archive set over a folder — but every one of those had the game
//! baked into it as a default. This module is where the game becomes a value
//! instead of a constant.
//!
//! A profile names: the game's master and DLC masters (the default load order),
//! the `SArchiveList` the game reads its assets from, which archive container
//! those are, and where that game's record schemas live. Everything else — the
//! TES4 container, the GRUP tree, the FormID numbering model — is shared, so a
//! profile is deliberately small.
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

    /// The default load order for this game, as file names.
    pub fn default_masters(&self) -> Vec<String> {
        self.masters.iter().map(|m| m.to_string()).collect()
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
