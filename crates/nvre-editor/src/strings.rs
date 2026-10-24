//! Finding a localized plugin's string tables on disk.
//!
//! [`nvre_esm::strings`] parses a table file; this module finds the files.
//! A plugin's tables are `<Stem>_<Language>.STRINGS` (with `.DLSTRINGS` and
//! `.ILSTRINGS` beside them), and they live in one of two places:
//!
//! - loose in `Data\Strings`, which is how Skyrim LE ships the base game's
//!   and Update.esm's;
//! - inside an archive — Skyrim keeps every plugin's tables in its
//!   `Interface` archive, and Skyrim LE's DLCs keep theirs in their own
//!   `Dawnguard.bsa` and kin [measured: all five SE plugins' English tables
//!   are in `Skyrim - Interface.bsa`; LE's are split between the loose
//!   `Strings` folder, that archive, and the three DLC archives].
//!
//! [`nvre_vfs::Vfs`] reads both container versions and the loose folder with
//! one case-insensitive path space, so both homes are one lookup. Only the
//! archives that can hold strings are opened — a session over an unlocalized
//! game (Fallout: New Vegas) opens none at all.

use nvre_esm::strings::{StringTable, StringTables, TableKind};
use nvre_esm::Plugin;
use nvre_vfs::Vfs;

/// The string tables of `plugins`, keyed by plugin file name.
///
/// Plugins whose TES4 header is not localized contribute nothing — their
/// names are text in their records — so a Fallout: New Vegas session pays
/// nothing for this. A localized plugin whose tables cannot be found
/// contributes no entry, and its names resolve to nothing rather than to
/// decoded string IDs (see [`StringTables::for_plugin`]).
pub fn load(
    game: &crate::game::GameProfile,
    data_dir: &std::path::Path,
    plugins: &[Plugin],
) -> StringTables {
    let localized: Vec<&Plugin> = plugins.iter().filter(|p| p.header.is_localized()).collect();
    let mut tables = StringTables::default();
    if localized.is_empty() {
        return tables;
    }
    let archives = string_archives(game, data_dir, &localized);
    let vfs = match Vfs::open(data_dir, &archives) {
        Ok(vfs) => vfs,
        Err(e) => {
            tracing::warn!(
                "{}: the string tables cannot be read ({e}); names from localized plugins will be missing",
                game.name
            );
            return tables;
        }
    };
    // Every table file the sources hold, as normalised lower-case paths.
    let files = vfs.paths(|p| p.starts_with("strings\\"));
    for plugin in localized {
        if let Some(table) = plugin_table(&vfs, &files, &plugin.name) {
            tables.insert(&plugin.name, table);
        }
    }
    tables
}

/// One plugin's three tables, merged.
///
/// The three share one ID space [measured: no ID appears in more than one of
/// `Skyrim_English`'s three files], so a lookup never needs to know which
/// file an ID came from. The language is the game's own when its tables are
/// among them, English otherwise, and the first found otherwise than that —
/// a real name in the wrong language beats no name at all.
fn plugin_table(vfs: &Vfs, files: &[String], plugin: &str) -> Option<StringTable> {
    let stem = plugin
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(plugin)
        .to_ascii_lowercase();
    let prefix = format!("{stem}_");
    let mut languages: Vec<&str> = files
        .iter()
        .filter_map(|path| {
            let name = path.rsplit('\\').next()?;
            let (base, ext) = name.rsplit_once('.')?;
            if !ext.eq_ignore_ascii_case("strings") {
                return None;
            }
            base.strip_prefix(prefix.as_str())
        })
        .collect();
    if languages.is_empty() {
        tracing::warn!("{plugin}: localized, but no string table was found for it");
        return None;
    }
    languages.sort_unstable();
    languages.dedup();
    // The language suffix is spelled per game: Skyrim's tables end
    // `_english`, Fallout 4's end `_en` [measured: `skyrim_english.strings`,
    // `fallout4_en.strings`]. English is preferred under either spelling, so
    // an English install shows English names rather than whichever language
    // sorts first.
    let language = languages
        .iter()
        .find(|l| l.eq_ignore_ascii_case("english") || l.eq_ignore_ascii_case("en"))
        .copied()
        .unwrap_or(languages[0]);
    let mut merged = StringTable::default();
    for (extension, kind) in [
        ("strings", TableKind::Strings),
        ("dlstrings", TableKind::Long),
        ("ilstrings", TableKind::Long),
    ] {
        let path = format!("strings\\{stem}_{language}.{extension}");
        let Some(bytes) = vfs.read(&path).and_then(Result::ok) else {
            continue;
        };
        match StringTable::parse(&bytes, kind) {
            Ok(table) => merged.merge(table),
            Err(e) => tracing::warn!("{plugin}: {path} cannot be read: {e}"),
        }
    }
    Some(merged)
}

/// The archives that can hold `plugins`' string tables, in precedence order.
///
/// The game's own `Interface` archive first — that is where Skyrim keeps the
/// tables — then each localized plugin's own archive, which is where Skyrim
/// LE's DLCs keep theirs. Only archives that are actually installed are
/// named, so a partial install opens what it has.
fn string_archives(
    game: &crate::game::GameProfile,
    data_dir: &std::path::Path,
    plugins: &[&Plugin],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let push = |name: String, out: &mut Vec<String>| {
        if data_dir.join(&name).exists() && !out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
    };
    for archive in game.archives {
        if archive.to_ascii_lowercase().contains("interface") {
            push(archive.to_string(), &mut out);
        }
    }
    for plugin in plugins {
        let stem = plugin
            .name
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(plugin.name.as_str());
        push(format!("{stem}.bsa"), &mut out);
        push(format!("{stem}.ba2"), &mut out);
    }
    out
}
