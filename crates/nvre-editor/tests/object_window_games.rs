//! What the object window shows, for every game the editor supports.
//!
//! The FNV-only test next door (`object_window.rs`) pins the shape of GECK's
//! window for the one game whose table was measured against GECK itself. This
//! one asks the same questions of every game, because the panel is one panel:
//! a game whose categories all fall into one group, or whose categories are
//! labelled with their own four-byte signature, is a game whose object window
//! does not work — and the panel cannot tell the difference on its own.
//!
//! What is asserted, per game:
//!
//! - the palette holds the number of objects the shipped master has, so a
//!   change to the grouping cannot quietly drop objects from the window;
//! - every category is labelled with a name, never with its raw signature;
//! - the categories are filed under more than one group, which is what makes
//!   a hundred-category palette navigable rather than one flat list.
//!
//! Every problem is collected before the test fails, so one run reports the
//! whole work list rather than the first thing it tripped over.
//!
//! The counts are measured from the shipped masters, base game only. A game
//! whose master is not installed is skipped rather than failed: the numbers
//! are about that game's data, and a machine without the game cannot check
//! them.

use nvre_editor::game::GameProfile;
use nvre_editor::session::Session;

/// What the shipped master holds, and how it must be presented.
struct Expect {
    /// The game's id, as [`GameProfile::id`] spells it.
    id: &'static str,
    /// Objects the palette must show.
    objects: usize,
    /// Categories it must file them under.
    categories: usize,
    /// The groups its categories must be filed under, when measured.
    ///
    /// FNV's eight are GECK's own; the other games' trees have not been
    /// transcribed yet, so only the fact that there is more than one group is
    /// asserted for them.
    groups: Option<&'static [&'static str]>,
}

/// Measured from each game's own master, base game only.
///
/// The Skyrim counts are one lower than the file's record count on purpose.
/// `Skyrim.esm` holds a `GMST` (`0x0123C00E`, `iDaysToRespawnVendor`) at top
/// level whose FormID names master index 1, and the file has no masters — so
/// the FormID does not resolve and the record is unreachable in the palette
/// for the same reason it is unreachable in the game. Both editions carry it.
const EXPECTED: &[Expect] = &[
    Expect {
        id: "fnv",
        objects: 44_550,
        categories: 95,
        groups: Some(&[
            "Actors",
            "Actor Data",
            "Items",
            "Magic",
            "Special Effects",
            "WorldObjects",
            "Audio",
            "Miscellaneous",
        ]),
    },
    Expect {
        id: "fo3",
        objects: 26_335,
        categories: 78,
        groups: None,
    },
    Expect {
        id: "skyrim",
        objects: 70_100,
        categories: 107,
        groups: None,
    },
    Expect {
        id: "skyrimse",
        objects: 70_101,
        categories: 107,
        groups: None,
    },
    Expect {
        id: "fo4",
        objects: 98_018,
        categories: 126,
        groups: None,
    },
];

/// Open a game's own master, or `None` when it is not installed.
fn open(game: &'static GameProfile) -> Option<Session> {
    let data = game.data_dir()?;
    let master = data.join(game.master);
    if !master.exists() {
        return None;
    }
    Session::open(
        &master,
        &game.schema_dirs().0,
        vec![game.master.to_string()],
    )
    .ok()
}

/// Everything wrong with one game's palette, as a list of lines.
fn problems(game: &'static GameProfile, want: &Expect, s: &mut Session) -> Vec<String> {
    let mut out = Vec::new();
    let groups = s.object_groups("").to_vec();
    let total: usize = groups.iter().map(|g| g.objects.len()).sum();

    // The window shows the game's objects. A grouping change that loses
    // objects is the failure this catches.
    if total != want.objects {
        out.push(format!(
            "{} objects in the palette, the master has {}",
            total, want.objects
        ));
    }
    if groups.len() != want.categories {
        out.push(format!(
            "{} categories, expected {}",
            groups.len(),
            want.categories
        ));
    }

    // A category is named the way the game's own editor names it. A category
    // still labelled with its signature is one the schema has no name for,
    // which shows the modder `MSTT` where the game says "Moveable Static".
    let mut unnamed: Vec<String> = groups
        .iter()
        .filter(|g| g.category == nvre_util::tag_str(&g.sig) || g.category.is_empty())
        .map(|g| nvre_util::tag_str(&g.sig))
        .collect();
    unnamed.sort_unstable();
    unnamed.dedup();
    if !unnamed.is_empty() {
        out.push(format!(
            "{} categories are labelled with their own signature: {}",
            unnamed.len(),
            unnamed.join(" ")
        ));
    }

    // One flat list is not a palette. Every category must be filed under a
    // group, and a game whose categories all land in the same group is a game
    // whose grouping table has not been written.
    let mut used: Vec<&str> = groups.iter().map(|g| g.group.as_str()).collect();
    used.sort_unstable();
    used.dedup();
    if used.len() <= 1 {
        out.push(format!(
            "every category is filed under {:?}, so the palette is one flat list of {} categories",
            used.first().copied().unwrap_or(""),
            groups.len()
        ));
    }

    // Where the game's tree has been measured, it is the tree the window must
    // show — the same names, and in the same order down the side.
    if let Some(order) = want.groups {
        let mut in_order: Vec<&str> = Vec::new();
        for g in &groups {
            if in_order.last() != Some(&g.group.as_str()) {
                in_order.push(g.group.as_str());
            }
        }
        if in_order != order {
            out.push(format!(
                "groups are listed as {:?}, expected {:?}",
                in_order, order
            ));
        }
    }
    let _ = game;
    out
}

#[test]
fn every_game_files_its_objects_into_named_groups() {
    let mut checked = 0;
    let mut failures: Vec<String> = Vec::new();
    for want in EXPECTED {
        let Some(game) = GameProfile::by_id(want.id) else {
            panic!("{} is not a game profile", want.id);
        };
        let Some(mut s) = open(game) else {
            eprintln!("skipped {}: its master is not installed", game.id);
            continue;
        };
        checked += 1;
        for p in problems(game, want, &mut s) {
            failures.push(format!("{}: {p}", game.id));
        }
    }
    assert!(checked > 0, "no game was installed, so nothing was checked");
    assert!(
        failures.is_empty(),
        "the object window does not work for every game:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn every_games_schema_validates_and_files_its_own_groups() {
    // The schema is the only table the panel reads now, and `Schema::check`
    // rejects a shown category filed under a heading its own group list does
    // not hold — which is what stops a game from inheriting another game's
    // heading. This needs no installed game: the schema chain loads from the
    // repository alone.
    for want in EXPECTED {
        let Some(game) = GameProfile::by_id(want.id) else {
            panic!("{} is not a game profile", want.id);
        };
        let schema = game
            .load_schema()
            .unwrap_or_else(|e| panic!("{}: schema does not load: {e}", game.id));
        assert!(
            !schema.object_window.groups.is_empty(),
            "{}: the object-window table names no groups",
            game.id
        );
        assert!(
            !schema.object_window.categories.is_empty(),
            "{}: the object-window table names no categories",
            game.id
        );
    }
}
