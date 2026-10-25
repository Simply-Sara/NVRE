//! What a **new** plugin depends on by default.
//!
//! The rule is one master, not every installed ESM. A plugin written against
//! the DLC will not load without them installed, so a new plugin that quietly
//! depends on five optional files is worse than one that depends on the base
//! game and is ticked up as the mod grows.
//!
//! These tests build their own folder rather than reading `testfiles/`: that
//! directory is gitignored, so a test that needs a file writes one.

use nvre_editor::game;
use nvre_editor::launcher::{default_masters, Launcher, LauncherTab};

/// A scratch folder with `names` written into it as empty `.esm` files.
///
/// The files are empty because `default_masters` only asks whether the base
/// master is *present*; it reads no header.
fn folder(name: &str, names: &[&str]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nvre-new-masters-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for n in names {
        std::fs::write(dir.join(n), b"").unwrap();
    }
    dir
}

/// A new plugin's default master is the game's own base master.
#[test]
fn the_base_master_is_the_default() {
    let dir = folder(
        "base",
        &["FalloutNV.esm", "DeadMoney.esm", "GunRunnersArsenal.esm"],
    );
    let masters = default_masters(&game::FNV, &dir);
    assert_eq!(masters, vec!["FalloutNV.esm".to_string()]);
}

/// The DLC are offered but not selected: they are optional content.
#[test]
fn the_dlc_are_not_default_masters() {
    let dir = folder(
        "dlc",
        &[
            "FalloutNV.esm",
            "DeadMoney.esm",
            "HonestHearts.esm",
            "OldWorldBlues.esm",
        ],
    );
    let masters = default_masters(&game::FNV, &dir);
    // Exactly one, and it is the base game rather than any of the four.
    assert_eq!(masters.len(), 1, "one default master, not {masters:?}");
    assert_eq!(masters[0], "FalloutNV.esm");
}

/// A portable install without the base master gets no default at all.
///
/// A master the plugin cannot open is a worse default than none: the launcher
/// would hand the session a `MAST` it cannot resolve.
#[test]
fn no_base_master_means_no_default() {
    let dir = folder("no-base", &["DeadMoney.esm", "SomeMod.esp"]);
    assert!(default_masters(&game::FNV, &dir).is_empty());
}

/// A folder that does not exist is the same answer, not an error.
#[test]
fn a_missing_folder_gives_no_default() {
    let dir = std::env::temp_dir().join("nvre-new-masters-absent");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(default_masters(&game::FNV, &dir).is_empty());
}

/// Every game's own base master, not a hard-coded Fallout: New Vegas one.
#[test]
fn each_game_gets_its_own_base_master() {
    let dir = folder("all-games", &["Fallout4.esm", "Skyrim.esm", "Fallout3.esm"]);
    assert_eq!(
        default_masters(&game::FALLOUT_4, &dir),
        vec!["Fallout4.esm"]
    );
    assert_eq!(default_masters(&game::SKYRIM, &dir), vec!["Skyrim.esm"]);
    assert_eq!(default_masters(&game::FO3, &dir), vec!["Fallout3.esm"]);
}

/// Switching to Create New gives the base master alone, not what Open
/// Existing had derived.
///
/// This is the regression: the tab switch was a bare field write, so the Create
/// New list inherited the Open Existing list — every installed ESM, all ticked —
/// until the game was switched away and back.
/// A master the user ticks is not dropped by the next frame.
///
/// `refresh_listing` runs every frame, and the launcher draws after it, so the
/// tick is applied to `masters` and the following frames must leave it alone.
/// `refresh_listing` short-circuits while the folder and order are unchanged,
/// which is what keeps the tick from being re-derived away.
#[test]
fn a_ticked_master_survives_the_next_frame() {
    let dir = folder("tick", &["FalloutNV.esm", "DeadMoney.esm"]);
    let mut launcher = Launcher::for_game(&game::FNV);
    launcher.set_tab(LauncherTab::CreateNew, &game::FNV, Some(&dir));
    assert_eq!(launcher.masters, vec!["FalloutNV.esm".to_string()]);

    // The user ticks DeadMoney. In the UI this is the checkbox handler pushing
    // the name onto `masters`.
    launcher.masters.push("DeadMoney.esm".to_string());

    // Several frames go by, each one calling `refresh_listing` as the launcher
    // does. None of them may re-derive the list.
    for _ in 0..3 {
        launcher.refresh_listing(&game::FNV, Some(&dir));
    }
    assert_eq!(
        launcher.masters,
        vec!["FalloutNV.esm".to_string(), "DeadMoney.esm".to_string()],
        "the tick must survive, not be re-derived away"
    );
}

/// Unticking works the same way: a list the user emptied is not refilled by the
/// frames that follow.
#[test]
fn an_unticked_master_stays_unticked() {
    let dir = folder("untick", &["FalloutNV.esm", "DeadMoney.esm"]);
    let mut launcher = Launcher::for_game(&game::FNV);
    launcher.set_tab(LauncherTab::CreateNew, &game::FNV, Some(&dir));

    launcher.masters.clear();
    for _ in 0..3 {
        launcher.refresh_listing(&game::FNV, Some(&dir));
    }
    assert!(launcher.masters.is_empty());
}

/// Switching to Create New gives the base master alone, not what Open
/// Existing had derived.
///
/// This is the regression: the tab switch was a bare field write, so the Create
/// New list inherited the Open Existing list — every installed ESM, all ticked —
/// until the game was switched away and back.
#[test]
fn switching_to_create_new_replaces_the_master_list() {
    let dir = folder(
        "tab-switch",
        &[
            "FalloutNV.esm",
            "DeadMoney.esm",
            "HonestHearts.esm",
            "SomeMod.esm",
        ],
    );
    let mut launcher = Launcher::for_game(&game::FNV);
    // The launcher starts on Open Existing, whose list is the folder's masters.
    launcher.refresh_listing(&game::FNV, Some(&dir));
    let open_existing = launcher.masters.clone();
    assert!(
        open_existing.len() > 1,
        "the Open Existing tab should show more than the base master"
    );

    launcher.set_tab(LauncherTab::CreateNew, &game::FNV, Some(&dir));
    assert_eq!(launcher.masters, vec!["FalloutNV.esm".to_string()]);
}

/// Switching back to Open Existing restores the folder's masters.
#[test]
fn switching_back_restores_the_derived_list() {
    let dir = folder(
        "tab-back",
        &["FalloutNV.esm", "DeadMoney.esm", "SomeMod.esm"],
    );
    let mut launcher = Launcher::for_game(&game::FNV);
    launcher.refresh_listing(&game::FNV, Some(&dir));
    let before = launcher.masters.clone();

    launcher.set_tab(LauncherTab::CreateNew, &game::FNV, Some(&dir));
    launcher.set_tab(LauncherTab::OpenExisting, &game::FNV, Some(&dir));
    assert_eq!(launcher.masters, before);
}
