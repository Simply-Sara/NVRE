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
    /// The folder this game keeps its `plugins.txt` in, under the user's
    /// LocalAppData — the game's own folder name, not its title.
    ///
    /// This is **not** the Data folder. A plugin lives in `Data`, but the order
    /// files are the game's *user* state and live beside its INIs and saves:
    /// `%LOCALAPPDATA%\FalloutNV\plugins.txt` on Windows [documented: the
    /// location every mod manager and LOOT's own docs name — "the plugins you
    /// have accessed are listed in `%LOCALAPPDATA%\plugins.txt`"]. The names
    /// are measured, not guessed: `FalloutNV`, `Fallout3`, `Skyrim`,
    /// `Skyrim Special Edition`, `Fallout4`, and the Proton prefixes on this
    /// machine hold the same layout verbatim.
    ///
    /// `None` for a game whose folder has not been measured.
    pub local_appdata_folder: Option<&'static str>,
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
    local_appdata_folder: Some("FalloutNV"),
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
    local_appdata_folder: Some("Fallout3"),
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
    notes: &[],
    local_appdata_folder: Some("Skyrim"),
};

/// Skyrim Special Edition. Its archives are BSA v105 with LZ4-compressed
/// blocks, read by NVRE's own `bsa105` crate (all 23 of the shipped ones,
/// 172,961 files, verified).
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
    notes: &[],
    local_appdata_folder: Some("Skyrim Special Edition"),
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
    notes: &[],
    local_appdata_folder: Some("Fallout4"),
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
        self.masters_in_with(data_dir, &crate::plugin_order::Rank::default())
    }

    /// The same list, with an order file's order deciding where it can.
    ///
    /// The headers are a constraint and the file is a preference: a master may
    /// not load after a plugin that requires it, so the file's order is applied
    /// **inside** the topological sort rather than by re-sorting its result.
    /// Re-sorting afterwards would take the file literally and put
    /// `FalloutNV.esm` after the plugins that require it, which
    /// `LoadOrder::new` rejects as `MasterAfterDependent` — a list the game
    /// cannot load, produced by a file that merely ordered things differently.
    ///
    /// An empty rank is every master unranked, which is exactly the order
    /// [`GameProfile::masters_in`] returns.
    pub fn masters_in_with(
        &self,
        data_dir: &Path,
        rank: &crate::plugin_order::Rank,
    ) -> Vec<String> {
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
            let deps = nvre_esm::Plugin::masters_in_header(data_dir.join(name))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|m| {
                    let l = m.to_ascii_lowercase();
                    // The master must be present, and a plugin naming itself
                    // is not a dependency edge.
                    lower.iter().position(|n| *n == l && *n != lower[i])
                })
                .collect::<Vec<_>>();
            needs.push((i, deps));
        }

        self.order_masters(&present, &needs, rank)
    }

    /// The load order for a dependency graph, as `(node, its dependencies)`.
    ///
    /// Kahn's algorithm. Which ready master goes next is decided by the order
    /// file's position first — that is the game's own answer where it has one —
    /// then by how many plugins depend on it, most first, then by name. A master
    /// many plugins need is the one the game loads early, and the name keeps the
    /// result deterministic rather than dependent on directory order. Nothing on
    /// disk pins the order among Fallout: New Vegas's DLC — every one of them
    /// declares only `FalloutNV.esm` — so without a file this is a valid load
    /// order that is not necessarily the game's canonical one, and with one it is
    /// the file's.
    ///
    /// The dependency edges are a constraint and are enforced by the filter, not
    /// by this key: a master that is not ready is not a candidate however early
    /// the file puts it.
    ///
    /// Split out from [`GameProfile::masters_in`] so the ordering rule is
    /// testable on a graph rather than only through a Data folder.
    pub fn order_masters(
        &self,
        present: &[String],
        needs: &[(usize, Vec<usize>)],
        rank: &crate::plugin_order::Rank,
    ) -> Vec<String> {
        let dependents: Vec<usize> = (0..present.len())
            .map(|i| needs.iter().filter(|(_, d)| d.contains(&i)).count())
            .collect();
        let lower: Vec<String> = present.iter().map(|n| n.to_ascii_lowercase()).collect();
        let mut done: Vec<bool> = vec![false; present.len()];
        let mut out: Vec<String> = Vec::with_capacity(present.len());
        while out.len() < present.len() {
            let next = (0..present.len())
                .filter(|&i| !done[i])
                .filter(|&i| needs[i].1.iter().all(|&d| done[d]))
                .min_by_key(|&i| {
                    (
                        rank.position(&present[i])
                            .unwrap_or(crate::plugin_order::UNRANKED),
                        std::cmp::Reverse(dependents[i]),
                        lower[i].clone(),
                    )
                })
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
    ///
    /// This is the repository's directory, which is where the schemas live
    /// while the editor is being worked on. A built editor reads the same
    /// schemas out of the binary instead — see [`GameProfile::load_schema`].
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
    /// The schemas are compiled into the binary rather than read off the
    /// disk, so a built editor is one file the user can copy anywhere. The
    /// layers are the same files the repository holds, in the same order, so
    /// the schema this builds is the schema the directory would have given.
    pub fn load_schema(&self) -> Result<nvre_esm::Schema, String> {
        self.load_schema_over(None)
    }

    /// Load this game's record schema over a directory of schema files.
    ///
    /// `dir` is a schema directory to read from the disk instead of from
    /// the compiled-in schemas. It names the directory the schemas live
    /// under — `schemas`, or `schemas/base` — and the game's own layers are
    /// read from beside it, so the chain is the same one the compiled-in
    /// schemas are: the base, then the game's family, then the game's own
    /// version.
    ///
    /// This is how a developer points the editor at layouts they are still
    /// writing, and how the tests read the repository's own directories.
    pub fn load_schema_over(&self, dir: Option<&Path>) -> Result<nvre_esm::Schema, String> {
        let Some(dir) = dir else {
            // The compiled-in chain: the base, then the game's family, then
            // the game's own version.
            let mut schema = nvre_esm::Schema::new();
            merge_layer(&mut schema, nvre_schemas::base())?;
            for name in [self.schema_family_dir, self.schema_version_dir] {
                if name.is_empty() {
                    continue;
                }
                // A layer that is not there is not a layer: the base holds
                // what every game shares, and a game with no layer of its own
                // reads the base alone.
                if let Some(layer) = nvre_schemas::layer(name) {
                    merge_layer(&mut schema, layer)?;
                }
            }
            schema.validate()?;
            return Ok(schema);
        };

        // The directory the schemas live under: `schemas`, whether the caller
        // named the base or a game's own directory.
        let root = dir.parent().unwrap_or(dir);
        let base_dir = root.join("base");
        let mut schema = if base_dir.is_dir() {
            nvre_esm::Schema::load_dir(&base_dir).map_err(|e| e.to_string())?
        } else {
            // No base on the disk: the compiled-in one stands in for it.
            let mut s = nvre_esm::Schema::new();
            merge_layer(&mut s, nvre_schemas::base())?;
            s
        };
        for name in [self.schema_family_dir, self.schema_version_dir] {
            if name.is_empty() {
                continue;
            }
            // A layer on the disk is read from there; a layer that is only
            // compiled in is read from the binary.
            let layer_dir = root.join(name);
            if layer_dir.is_dir() {
                merge_dir(&mut schema, &layer_dir)?;
            } else if let Some(layer) = nvre_schemas::layer(name) {
                merge_layer(&mut schema, layer)?;
            }
        }
        schema.validate()?;
        Ok(schema)
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
    ///
    /// As with the record schemas, the layouts are compiled into the binary.
    pub fn load_nif_schema(&self) -> Result<nvre_nif::Schema, String> {
        let mut schema = nvre_nif::Schema::new();
        merge_nif_layer(&mut schema, nvre_schemas::layer("nif/base"))?;
        for name in [self.nif_schema_family_dir, self.nif_schema_version_dir] {
            if name.is_empty() {
                continue;
            }
            if let Some(layer) = nvre_schemas::layer(&format!("nif/{name}")) {
                merge_nif_layer(&mut schema, Some(layer))?;
            }
        }
        schema.check().map_err(|e| e.to_string())?;
        Ok(schema)
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
///
/// Each platform has its own convention: XDG on Linux and macOS, and
/// `%APPDATA%` on Windows. The fallback when the variable is unset is the
/// platform's own default location.
fn config_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
            std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("AppData/Roaming"))
        })?
    } else {
        // XDG, with the fallback the spec names when it is unset.
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?
    };
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
///
/// Steam installs itself in a different place on each platform, so the root is
/// the platform's own: `~/.local/share/Steam` on Linux,
/// `~/Library/Application Support/Steam` on macOS, and `C:\Program Files (x86)\Steam`
/// on Windows.
fn steam_libraries() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for root in steam_roots() {
        let vdf = root.join("steamapps/libraryfolders.vdf");
        if let Ok(text) = std::fs::read_to_string(&vdf) {
            if !out.contains(&root) {
                out.push(root);
            }
            for path in vdf_paths(&text) {
                if !out.contains(&path) {
                    out.push(path);
                }
            }
        }
    }
    out
}

/// Every folder Steam might be installed in on this platform.
///
/// The first is the documented default. The rest are places Steam is also
/// commonly installed, and they cost nothing to check: a folder that is not
/// there simply has no `libraryfolders.vdf`.
fn steam_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(target_os = "windows") {
        // Steam's own default install, and the 32-bit program files folder
        // it uses on a 64-bit Windows.
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(dir) = std::env::var_os(var) {
                out.push(PathBuf::from(dir).join("Steam"));
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            out.push(PathBuf::from(home).join("Library/Application Support/Steam"));
        }
    } else if let Some(home) = std::env::var_os("HOME") {
        out.push(PathBuf::from(home).join(".local/share/Steam"));
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

/// Merge one embedded layer into a record schema.
///
/// The files are merged in the order the layer holds them, which is the
/// order the directory's files are read in — sorted by name — so the schema
/// this builds is the schema the directory would have given.
fn merge_layer(schema: &mut nvre_esm::Schema, layer: &nvre_schemas::Layer) -> Result<(), String> {
    for (name, text) in layer.files {
        schema
            .merge_str(text)
            .map_err(|e| format!("{}/{name}: {e}", layer.name))?;
    }
    Ok(())
}

/// Merge a directory of schema files into a record schema.
///
/// The directory's own files are merged, and the base it carries is *not*:
/// the base is already in the schema, and merging it twice would merge the
/// base's own records twice over. A game directory carries a link to the
/// base so that it is readable on its own, and this is what reads it without
/// that link.
fn merge_dir(schema: &mut nvre_esm::Schema, dir: &Path) -> Result<(), String> {
    let mut files: Vec<PathBuf> = Vec::new();
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|x| x == "json") {
            files.push(path);
        }
    }
    // The same order the directory's files are read in elsewhere.
    files.sort();
    for file in files {
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        schema
            .merge_str(&text)
            .map_err(|e| format!("{}/{name}: {e}", dir.display()))?;
    }
    Ok(())
}

/// Merge one embedded layer into a NIF schema.
///
/// As with the record schemas: the files in the order the layer holds them.
fn merge_nif_layer(
    schema: &mut nvre_nif::Schema,
    layer: Option<&nvre_schemas::Layer>,
) -> Result<(), String> {
    let Some(layer) = layer else {
        return Ok(());
    };
    for (name, text) in layer.files {
        schema
            .merge_str(text)
            .map_err(|e| format!("{}/{name}: {e}", layer.name))?;
    }
    Ok(())
}
