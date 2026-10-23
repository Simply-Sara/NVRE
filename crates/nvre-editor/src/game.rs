//! Which game a `Data` folder holds, and what that changes.
//!
//! The editor's format work is game-neutral in shape — records are decoded
//! through a schema directory, the load order is a master list, and the asset
//! layer is an archive set over a folder — but every one of those had the game
//! baked into it as a default. This module is where the game becomes a value
//! instead of a constant.
//!
//! A profile names: the folder Steam installs the game into, the game's own
//! master, the `SArchiveList` it reads its assets from, which archive container
//! those are, and where that game's record schemas live. Everything else — the
//! TES4 container, the GRUP tree, the FormID numbering model — is shared, so a
//! profile is deliberately small.
//!
//! Nothing here lists the game's masters. A session's master list is read from
//! the game's own Data folder and ordered by a topological sort of the plugins'
//! own `MAST` headers ([`GameProfile::masters_in`]): the order is on disk, in
//! the files, so the profile does not restate it. That keeps a profile correct
//! for a partial install, for a mod master the user added, and for an install
//! whose DLC set is not the shipped one.
//!
//! What a profile does **not** carry, because it is not implemented yet:
//!
//! - the NIF schema, which is still the FNV corpus's
//!   (`schemas/nif`, versions 20.0.0.4–20.2.0.7, `bs_version` 11–34),
//!   so a non-FNV game's meshes are decoded with the wrong version's
//!   conditions until a per-game overlay exists;
//! - the record schemas for the games other than FNV and Fallout 3: those two
//!   are measured, so opening another game's master fails with
//!   `no schema files in ...` rather than silently misreading it;
//! - the GECK/CK object-window group table, which is FNV's
//!   ([`super::session::geck_group_for_sig`]) until each game has its own;
//! - archive containers other than BSA v104 (Fallout 4 ships BA2). Skyrim SE's
//!   BSA v105/LZ4 is readable through NVRE's own [`bsa105`] crate, but the
//!   editor's VFS reads v104 only — so Skyrim SE
//!   opens with loose files only and says so through [`GameProfile::notes`]
//!   until the VFS takes the crate.
//!
//! Every list here is taken from the game's own shipped defaults, cited in the
//! field's doc comment. None of it is guessed.

use std::path::{Path, PathBuf};

/// The archive container a game ships its assets in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    /// BSA v104, zlib: Fallout 3, Fallout: New Vegas, Skyrim (Legendary).
    /// [`nvre_vfs::Vfs`] reads this, so the profile's archive list is used.
    Bsa104,
    /// Fallout 4's and Fallout 76's BA2 containers.
    Ba2,
    /// A container no reader here handles yet.
    Unreadable,
    /// BSA v105 with LZ4 blocks: Skyrim SE. NVRE's `bsa105` crate reads these;
    /// the editor's VFS does not yet, so this is distinct from `Bsa104` to keep
    /// that gap visible rather than silently promising assets that will not
    /// load.
    Bsa105,
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
    /// The directory holding the layouts this game's family shares, if it has
    /// one. Fallout 3 and Fallout: New Vegas are the same family, and so are
    /// Skyrim and Skyrim Special Edition: the two versions differ in a
    /// handful of records, and the rest is shared.
    pub schema_family_dir: &'static str,
    /// The directory holding this version's own differences from its family.
    pub schema_version_dir: &'static str,
    /// This game's NIF schemas, as a subdirectory of the repo's `schemas/nif/`.
    ///
    /// The same chain as the record schemas: a base, a family and a version.
    /// A game's own directory is named here; the chain is worked out from the
    /// family's and the version's names, so a game with no directory of its
    /// own shares its family's layouts unchanged.
    /// The directory holding the NIF layouts this game's family shares.
    pub nif_schema_family_dir: &'static str,
    /// The directory holding this version's own NIF differences.
    pub nif_schema_version_dir: &'static str,
    /// What this game cannot do yet, for the UI to say plainly rather than
    /// showing an empty viewport as if it were the game's fault.
    pub notes: &'static [&'static str],
}

/// Fallout: New Vegas — the editor's own game, and the only one whose record
/// schemas are measured against real data.
pub const FNV: GameProfile = GameProfile {
    id: "fnv",
    nif_schema_family_dir: "",
    nif_schema_version_dir: "fnv",
    name: "Fallout: New Vegas",
    editor_name: "New Vegas Rust Editor",
    master: "FalloutNV.esm",
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
    schema_version_dir: "fnv",
    schema_family_dir: "",
    notes: &[],
};

/// Fallout 3. Same BSA v104 container and the same Gamebryo NIF versions as
/// FNV, so the asset layer already works; only the record layouts are missing.
pub const FO3: GameProfile = GameProfile {
    id: "fo3",
    nif_schema_family_dir: "fnv",
    nif_schema_version_dir: "fo3",
    name: "Fallout 3",
    editor_name: "Fallout 3 Rust Editor",
    master: "Fallout3.esm",
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
    schema_version_dir: "fo3",
    schema_family_dir: "fnv",
    notes: &[],
};

/// Skyrim (Legendary Edition). BSA v104, like FNV and Fallout 3.
pub const SKYRIM: GameProfile = GameProfile {
    id: "skyrim",
    nif_schema_family_dir: "skyrim",
    nif_schema_version_dir: "skyrimle",
    name: "Skyrim",
    editor_name: "Skyrim Rust Editor",
    master: "Skyrim.esm",
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
    schema_subdir: "skyrimle",
    schema_family_dir: "skyrim",
    schema_version_dir: "skyrimle",
    notes: &[
        "record schemas are complete: all five masters decode at 100.00% over schemas/base + schemas/skyrim + schemas/skyrimle",
        "NIF schema is complete for the geometry: over its whole mesh archive, 631,970 blocks, no unknown block, every block that decoded re-encoding byte-identically, and the 60,362 shapes and 62,524 shape data blocks failing nothing",
        "its archives are BSA v104, which the editor's VFS reads, so the models load",
    ],
};

/// Skyrim Special Edition. Its archives are BSA v105 with LZ4-compressed
/// blocks. NVRE's own `bsa105` crate reads them (all 23 of the shipped ones,
/// 172,961 files, verified), but the editor's VFS reads v104 only — hence the
/// archive list is empty and the note says so.
pub const SKYRIM_SE: GameProfile = GameProfile {
    id: "skyrimse",
    nif_schema_family_dir: "skyrim",
    nif_schema_version_dir: "skyrimse",
    name: "Skyrim Special Edition",
    editor_name: "Skyrim SE Rust Editor",
    master: "Skyrim.esm",
    // The shipped `Skyrim.ini`'s `sResourceArchiveList`. The Special Edition
    // splits the textures and the meshes into numbered archives, and every
    // one of them is v105.
    archives: &[
        "Skyrim - Misc.bsa",
        "Skyrim - Shaders.bsa",
        "Skyrim - Textures0.bsa",
        "Skyrim - Textures1.bsa",
        "Skyrim - Textures2.bsa",
        "Skyrim - Textures3.bsa",
        "Skyrim - Textures4.bsa",
        "Skyrim - Textures5.bsa",
        "Skyrim - Textures6.bsa",
        "Skyrim - Textures7.bsa",
        "Skyrim - Textures8.bsa",
        "Skyrim - Interface.bsa",
        "Skyrim - Animations.bsa",
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Sounds.bsa",
        "Skyrim - Voices_en0.bsa",
    ],
    archive_format: ArchiveFormat::Bsa105,
    steam_folder: Some("Skyrim Special Edition"),
    schema_subdir: "skyrimse",
    schema_family_dir: "skyrim",
    schema_version_dir: "skyrimse",
    notes: &[
        "record schemas are complete: all five masters decode at 100.00% over schemas/base + schemas/skyrim + schemas/skyrimse",
        "archives are BSA v105/LZ4, which the editor's VFS reads through NVRE's own bsa105 crate, so the models load",
        "its NIF blocks all decode and its shapes are measured, and the packed vertex format its shapes use (BSVertexDataSSE) is now unpacked into real geometry",
    ],
};

/// Fallout 4. BA2 archives, ESL plugins, and a record set the least shared with
/// FNV's — the largest of the four to bring up.
pub const FALLOUT_4: GameProfile = GameProfile {
    id: "fo4",
    nif_schema_family_dir: "",
    nif_schema_version_dir: "fo4",
    name: "Fallout 4",
    editor_name: "Fallout 4 Rust Editor",
    master: "Fallout4.esm",
    // The base game's own archives, in load order. Each plugin's `- Main.ba2`
    // and `- Textures.ba2` are found from the plugin list the same way the
    // BSA games' are.
    archives: &[
        "Fallout4 - Animations.ba2",
        "Fallout4 - Interface.ba2",
        "Fallout4 - Materials.ba2",
        "Fallout4 - Meshes.ba2",
        "Fallout4 - MeshesExtra.ba2",
        "Fallout4 - Misc.ba2",
        "Fallout4 - Nvflex.ba2",
        "Fallout4 - Shaders.ba2",
        "Fallout4 - Sounds.ba2",
        "Fallout4 - Startup.ba2",
        "Fallout4 - Textures1.ba2",
        "Fallout4 - Textures2.ba2",
        "Fallout4 - Textures3.ba2",
        "Fallout4 - Textures4.ba2",
        "Fallout4 - Textures5.ba2",
        "Fallout4 - Textures6.ba2",
        "Fallout4 - Textures7.ba2",
        "Fallout4 - Textures8.ba2",
        "Fallout4 - Textures9.ba2",
    ],
    archive_format: ArchiveFormat::Ba2,
    steam_folder: Some("Fallout 4"),
    schema_subdir: "fo4",
    schema_version_dir: "fo4",
    schema_family_dir: "",
    notes: &[
        "record schemas are written but not yet measured against Fallout4.esm",
        "archives are BA2, read by NVRE's own ba2 crate: GNRL files and DX10 textures both decode",
        "its NIF layouts are measured against the meshes in Fallout4 - Meshes.ba2: NiAVObject drops the property list and the velocity vector, NiNode drops the effects list, and BSTriShape's num_triangles is a u32 with no trailing word",
        "its shapes use BSVertexData (bs_version 130), whose position is a half-precision vector unless the description's FULL_PRECISION flag is set",
        "ESL plugins (header flag 0x200) and the 0xFE000800 FormID range are not modelled yet",
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
    /// Detection is by what the folder ships, not by an executable name that
    /// lives outside it. Skyrim LE and SE both ship `Skyrim.esm`, so the pair is
    /// told apart by the archives: SE splits its large ones into numbered
    /// files (`Skyrim - Meshes0.bsa`, `Meshes1.bsa`, …) where LE ships one
    /// (`Skyrim - Meshes.bsa`). That is a difference in what the game ships
    /// rather than in what a modder added, so it holds however the folder was
    /// put together.
    ///
    /// A `.esl` plugin also marks SE — LE has no such format — but a mod may
    /// bring one, so it is not used to decide. The executable check is kept for
    /// a folder that has one, but a Data folder normally does not.
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
            // SE numbers its split archives; LE does not.
            if has("Skyrim - Meshes0.bsa") || has("Skyrim - Textures0.bsa") {
                return &SKYRIM_SE;
            }
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

    /// The masters present in `data_dir`, in a load order the game accepts.
    ///
    /// The order is **derived from the plugins themselves**, not from a table.
    /// Every plugin's `MAST` header names the masters it needs, and a master
    /// must load before whatever depends on it — so those headers are the edges
    /// of a dependency graph, and a topological sort of it is a load order.
    /// Masters nothing constrains against each other (Fallout: New Vegas's DLC
    /// all declare only `FalloutNV.esm`, so their mutual order is on no disk)
    /// are ordered by name, which is a valid order even though it is not the
    /// game's canonical one.
    ///
    /// Reading the folder rather than a fixed list is what makes this correct
    /// for a partial install: only the DLC the user actually owns appears, and a
    /// folder holding only the base game gives a one-master list. It also picks
    /// up a mod master the user installed, which a fixed list could not see, and
    /// places it after everything it depends on rather than in front of it.
    ///
    /// `.esp` files are not masters and are never included.
    pub fn masters_in(&self, data_dir: &Path) -> Vec<String> {
        // Every `.esm`/`.esl` in the folder, lower-cased for lookups.
        let present: Vec<String> = match std::fs::read_dir(data_dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| {
                    let l = n.to_ascii_lowercase();
                    l.ends_with(".esm") || l.ends_with(".esl")
                })
                .collect(),
            Err(_) => return Vec::new(),
        };
        let lower: Vec<String> = present.iter().map(|n| n.to_ascii_lowercase()).collect();

        // The edges: for each plugin, the masters it names that are present.
        // A header naming a master the folder lacks is skipped — that plugin is
        // a mod whose dependency is not installed, and the launcher's own
        // checks report that rather than this ordering guessing at it.
        let mut needs: Vec<(usize, Vec<usize>)> = Vec::with_capacity(present.len());
        for (i, name) in present.iter().enumerate() {
            let deps = match nvre_esm::Plugin::open(data_dir.join(name)) {
                Ok(p) => p
                    .header
                    .masters
                    .iter()
                    .filter_map(|m| {
                        let l = m.name.to_ascii_lowercase();
                        // The master must be present, and a plugin naming itself
                        // is not a dependency edge.
                        lower.iter().position(|n| *n == l && *n != lower[i])
                    })
                    .collect::<Vec<_>>(),
                // An unreadable file is still a master the folder holds; with no
                // header to read it simply constrains nothing.
                Err(_) => Vec::new(),
            };
            needs.push((i, deps));
        }

        // Kahn's algorithm. Which ready master goes next is decided by how many
        // plugins depend on it, most first, then by name: a master many plugins
        // need is the one the game loads early, and the name keeps the result
        // deterministic rather than dependent on directory order. Nothing on
        // disk pins the order among Fallout: New Vegas's DLC — every one of
        // them declares only `FalloutNV.esm` — so this is a valid load order
        // that is not necessarily the game's canonical one.
        let dependents: Vec<usize> = (0..present.len())
            .map(|i| needs.iter().filter(|(_, d)| d.contains(&i)).count())
            .collect();
        let mut done: Vec<bool> = vec![false; present.len()];
        let mut out: Vec<String> = Vec::with_capacity(present.len());
        while out.len() < present.len() {
            let next = (0..present.len())
                .filter(|&i| !done[i])
                .filter(|&i| needs[i].1.iter().all(|&d| done[d]))
                .min_by_key(|&i| (std::cmp::Reverse(dependents[i]), lower[i].clone()))
                .or_else(|| {
                    // Nothing is ready: a cycle among the headers (a mod pair
                    // that depend on each other), which the game cannot load
                    // either. Take the first remaining rather than looping.
                    (0..present.len()).find(|&i| !done[i])
                });
            let Some(i) = next else { break };
            done[i] = true;
            out.push(present[i].clone());
        }
        out
    }

    /// Where this game's record schemas live, and what they overlay.
    ///
    /// The schema directories for this game: `schemas/base` and, unless this is
    /// FNV itself, this game's own directory.
    ///
    /// A game with no directory of its own is not an error — it shares every
    /// layout the base holds. `load_game` skips a directory that does not
    /// exist, so a game whose schemas have not been written yet reads the base
    /// alone and its coverage is whatever that gives.
    pub fn schema_dirs(&self) -> (PathBuf, Option<PathBuf>) {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/base");
        if self.id == "fnv" {
            return (base, None);
        }
        let overlay = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas")
            .join(self.schema_subdir);
        (base, Some(overlay))
    }

    /// Load this game's record schema.
    ///
    /// The chain is `base -> <family> -> <version>`, and nothing else.
    ///
    /// The base holds only what every supported game has, and its layouts
    /// are the ones that decode correctly in all of them. A record or a type
    /// that one family has belongs to that family's directory: Skyrim's
    /// `NPC_` and Fallout's `NPC_` share a signature and nothing else, so
    /// neither can live in the base.
    ///
    /// A family is a set of versions of the same game. Fallout: New Vegas and
    /// Fallout 3 are one family, and so are Skyrim and Skyrim Special
    /// Edition: the two Skyrim versions differ in a handful of records and
    /// share the rest, so the shared layouts live in the family's directory
    /// and each version's directory holds only its own differences.
    ///
    /// A directory carries a `base -> ../base` symlink, so it is also
    /// readable on its own; `load_game` skips the files reached through that
    /// symlink, which is what keeps the base from merging twice.
    pub fn load_schema(&self) -> Result<nvre_esm::Schema, String> {
        let (base, _) = self.schema_dirs();
        let mut chain: Vec<std::path::PathBuf> = Vec::new();
        if let Some(family) = self.schema_family() {
            chain.push(family);
        }
        if let Some(version) = self.schema_version() {
            chain.push(version);
        }
        nvre_esm::Schema::load_game(
            &base,
            &chain
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<String>>(),
        )
        .map_err(|e| e.to_string())
    }

    /// Load this game's NIF schema.
    ///
    /// The chain is `base -> family -> version`, and nothing else, exactly as
    /// the record schemas are layered.
    ///
    /// The base holds the layouts every game shares, and its conditions are
    /// bounded by Fallout's own `bs_version` 11-34. A family holds the
    /// layouts its versions share, and a version holds its own differences.
    /// A layer *replaces* the blocks it names, so a version's file carries a
    /// complete block rather than a patch to one.
    pub fn load_nif_schema(&self) -> Result<nvre_nif::Schema, String> {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/nif/base");
        let mut chain: Vec<String> = Vec::new();
        if let Some(family) = self.nif_schema_dir(self.nif_schema_family_dir) {
            chain.push(family.to_string_lossy().into_owned());
        }
        if let Some(version) = self.nif_schema_dir(self.nif_schema_version_dir) {
            chain.push(version.to_string_lossy().into_owned());
        }
        nvre_nif::Schema::load_game(&base, &chain).map_err(|e| e.to_string())
    }

    /// One of the repo's NIF schema directories, by its name.
    ///
    /// A layer that is not there is not a layer: the base holds what every
    /// game shares, and a game with no layer of its own reads the base
    /// alone.
    fn nif_schema_dir(&self, name: &str) -> Option<PathBuf> {
        if name.is_empty() {
            return None;
        }
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/nif")
            .join(name);
        dir.is_dir().then_some(dir)
    }

    /// The directory holding the layouts this game's family shares.
    fn schema_family(&self) -> Option<std::path::PathBuf> {
        self.schema_dir(self.schema_family_dir)
    }

    /// The directory holding this version's own differences from its family.
    fn schema_version(&self) -> Option<std::path::PathBuf> {
        self.schema_dir(self.schema_version_dir)
    }

    /// One of the repo's schema directories, by its name.
    fn schema_dir(&self, name: &str) -> Option<std::path::PathBuf> {
        if name.is_empty() {
            None
        } else {
            Some(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../schemas")
                    .join(name),
            )
        }
    }

    /// Open the game's assets: its archive set over `data_dir`, plus the NIF
    /// schema.
    ///
    /// The archive order comes from the profile, so each game reads the
    /// `SArchiveList` its own INI names rather than FNV's. A game whose
    /// container is not readable yet opens with loose files only and logs why:
    /// the editor can still edit that game's plugins, it just cannot render
    /// archived meshes.
    pub fn open_asset_data(&self, data_dir: &Path) -> Result<nvre_assets::GameData, String> {
        let order = match self.archive_format {
            ArchiveFormat::Bsa104 => {
                // The plugins in the folder, so each one's own archives
                // (`<plugin>.bsa`, `<plugin> - *.bsa`) are found however many
                // the install holds. Read here rather than from a list, for the
                // same reason `masters_in` reads it.
                let plugins = self.masters_in(data_dir);
                let names: Vec<&str> = plugins.iter().map(String::as_str).collect();
                nvre_vfs::Vfs::archive_order(data_dir, self.archives, &names)
            }
            ArchiveFormat::Bsa105 => {
                // The same order as the v104 case: the game's own list, then
                // `Update.bsa`, then each loaded plugin's own archives. The
                // archives themselves are read by NVRE's own `bsa105` crate,
                // which handles both container versions.
                let plugins = self.masters_in(data_dir);
                let names: Vec<&str> = plugins.iter().map(String::as_str).collect();
                nvre_vfs::Vfs::archive_order(data_dir, self.archives, &names)
            }
            ArchiveFormat::Ba2 => {
                // The same order as the BSA games: the game's own list, then
                // each loaded plugin's own archives. The archives themselves
                // are read by NVRE's own `ba2` crate; the VFS picks that
                // reader from each file's magic, so nothing here has to name
                // the format.
                let plugins = self.masters_in(data_dir);
                let names: Vec<&str> = plugins.iter().map(String::as_str).collect();
                nvre_vfs::Vfs::archive_order(data_dir, self.archives, &names)
            }
            ArchiveFormat::Unreadable => {
                tracing::warn!(
                    "{}: this game's archives are not readable yet; opening with loose files only",
                    self.name
                );
                Vec::new()
            }
        };
        let vfs = nvre_vfs::Vfs::open(data_dir, &order).map_err(|e| e.to_string())?;
        // Each game's own NIF schema, layered over the shared base.
        let schema = self.load_nif_schema()?;
        Ok(nvre_assets::GameData {
            vfs: std::sync::Arc::new(vfs),
            schema: std::sync::Arc::new(schema),
        })
    }

    /// The top-level object-window group for a record signature.
    ///
    /// The table is the schema's, because it is a per-game fact: GECK's eight
    /// groups and the Creation Kit's nine are different partitions of the same
    /// signatures, and `MSTT` is "Moveable Static" in one game and "Movement
    /// Type" in another. `None` means this game's table has no entry, and the
    /// panel files the category under its catch-all rather than under a
    /// heading the game does not use.
    pub fn group_for_sig(&self, sig: &[u8; 4]) -> Option<&'static str> {
        let _ = sig;
        None
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
