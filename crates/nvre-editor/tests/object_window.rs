//! What the Objects panel's categories hold.
//!
//! GECK's Object Window lists categories on the left and that category's objects
//! on the right, so the panel's data is grouped the same way: one group per
//! placeable record type, named the way GECK names it. Getting the grouping
//! wrong either merges two categories into one or drops objects from a category
//! entirely, so both are checked against the game's own load order.

use nvre_editor::session::Session;

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

fn open_master() -> Option<Session> {
    let data = data_dir()?;
    Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}

#[test]
fn the_palette_is_grouped_into_gecks_categories() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let groups = s.object_groups("").to_vec();
    // The load order's placeable objects, all of them: the panel used to show
    // 200 of 11,483 and say so, which made everything past the first 200
    // alphabetically unreachable.
    let total: usize = groups.iter().map(|g| g.objects.len()).sum();
    assert_eq!(total, 44_550, "every object is reachable");

    // All categories belong to one of GECK's 8 top-level groups.
    for g in &groups {
        assert!(
            [
                "Actors",
                "Actor Data",
                "Items",
                "Magic",
                "Special Effects",
                "WorldObjects",
                "Audio",
                "Miscellaneous"
            ]
            .contains(&g.group.as_str()),
            "{} is in a valid GECK group ({})",
            g.category,
            g.group
        );
    }

    // Actor Data group exists and holds actor data categories like Quest, Perk, Faction.
    let actor_data_cats: Vec<&str> = groups
        .iter()
        .filter(|g| g.group == "Actor Data")
        .map(|g| g.category.as_str())
        .collect();
    assert!(actor_data_cats.contains(&"Quest"));
    assert!(actor_data_cats.contains(&"Perk"));
    assert!(actor_data_cats.contains(&"Faction"));
    assert!(actor_data_cats.contains(&"Race"));

    // A category is named the way GECK names it, never by raw signature.
    for g in &groups {
        assert_ne!(g.category, "STAT");
        assert_ne!(g.category, "ACTI");
        assert_ne!(g.category, "WEAP");
    }

    // What the schema marks hidden is not listed: the palette holds no
    // placed references, cells, worldspaces or dialogues — hiding them is
    // the table's answer now, not a list in the code beside it.
    for g in &groups {
        let cat = s
            .schema
            .object_window
            .categories
            .get(&nvre_util::tag_str(&g.sig))
            .expect("a listed category is in the table");
        assert!(!cat.hidden, "{} is listed but marked hidden", g.category);
    }

    // Each object is in exactly one category, and its own base object's record
    // type is the one it is filed under.
    let lo = s.load_order().expect("a load order");
    for g in &groups {
        for row in &g.objects {
            let sig = nvre_world::fields::winner(lo, row.fid)
                .map(|(_, rec)| rec.header.sig)
                .expect("the base object");
            let name = s
                .schema
                .records
                .get(String::from_utf8_lossy(&sig).as_ref())
                .map(|d| d.name.clone())
                .expect("the record type has a name");
            assert_eq!(
                name, g.category,
                "{:08X} is filed under {name}, not {}",
                row.fid, g.category
            );
        }
    }

    // Objects are sorted by name inside a category, which is what makes a
    // 6,785-strong category navigable.
    for g in &groups {
        let sorted = g.objects.windows(2).all(|w| w[0].label() <= w[1].label());
        assert!(sorted, "{}'s objects are sorted by name", g.category);
    }
}

#[test]
fn a_filter_narrows_both_halves_of_the_window() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let all = s.object_groups("").to_vec();
    let filtered = s.object_groups("dumpster").to_vec();

    // Every match is still there, in the category its base object belongs to —
    // the filter narrows the list, it does not merge the categories.
    let kept: usize = filtered.iter().map(|g| g.objects.len()).sum();
    assert!(kept > 0, "the filter matches something");
    let wanted: Vec<&nvre_editor::session::ObjectRow> = all
        .iter()
        .flat_map(|g| g.objects.iter())
        .filter(|row| row.label().to_lowercase().contains("dumpster"))
        .collect();
    assert_eq!(
        kept,
        wanted.len(),
        "the filter keeps exactly the objects whose name matches"
    );
    for g in &filtered {
        for row in &g.objects {
            assert!(
                row.label().to_lowercase().contains("dumpster"),
                "{:08X} ({}) does not match the filter",
                row.fid,
                row.label()
            );
        }
    }

    // A category with nothing left in it is dropped rather than shown empty, so
    // the left half of the window does not list categories that cannot be
    // picked.
    assert!(
        filtered.iter().all(|g| !g.objects.is_empty()),
        "an empty category is dropped"
    );
    assert!(
        filtered.len() < all.len(),
        "the filter narrows the category list too"
    );
}

#[test]
fn a_filter_that_matches_nothing_leaves_an_empty_palette() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // The panel says "no objects match" rather than showing an empty category.
    let groups = s.object_groups("zzzznotanobject").to_vec();
    assert!(groups.is_empty(), "nothing matches, so there are no groups");
}

#[test]
fn placing_objects_and_actors_creates_correct_record_types() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let cell = s
        .index
        .cells
        .values()
        .find(|c| c.edid.as_deref() == Some("Goodsprings"))
        .map(|c| c.form_id)
        .expect("Goodsprings");
    s.open_cell(cell);

    let groups = s.object_groups("").to_vec();

    // Place a weapon (WEAP) -> should insert a REFR
    let weapons = groups
        .iter()
        .find(|g| g.category == "Weapon")
        .expect("Weapon category");
    let weap_fid = weapons.objects[0].fid;
    s.place(weap_fid, [100.0, 200.0, 300.0]);
    let placed_weap = s.selected_record().expect("placed weapon");
    assert_eq!(placed_weap.record().header.sig, *b"REFR");

    // Place an NPC (NPC_) -> should insert an ACHR
    let npcs = groups
        .iter()
        .find(|g| g.category == "Non-Player Character")
        .expect("NPC category");
    let npc_fid = npcs.objects[0].fid;
    s.place(npc_fid, [10.0, 20.0, 30.0]);
    let placed_npc = s.selected_record().expect("placed npc");
    assert_eq!(placed_npc.record().header.sig, *b"ACHR");

    // Select a non-placeable record (e.g. Quest) -> selected_record should resolve it
    let quests = groups
        .iter()
        .find(|g| g.category == "Quest")
        .expect("Quest category");
    let quest_fid = quests.objects[0].fid;
    s.select(quest_fid);
    let selected_quest = s.selected_record().expect("selected quest");
    assert_eq!(selected_quest.record().header.sig, *b"QUST");
}

/// The FNV schema accounts for every record type it defines: each one is
/// either filed under a group or named as hidden, and each shown one names
/// an editor block this build registers.
///
/// This is the "explicit only" half of the object window, and it needs no
/// installed game — the schema chain loads from the repository alone. A
/// record type with no entry would be invisible in the window with nothing
/// saying why; a block name with no registered page would open an editor
/// that silently shows nothing.
#[test]
fn fnv_schema_lists_every_record_type_and_names_a_registered_block() {
    let game = nvre_editor::game::GameProfile::by_id("fnv").expect("fnv profile");
    let schema = game.load_schema().expect("the FNV schema loads");

    let mut missing: Vec<&str> = schema
        .records
        .keys()
        .filter(|s| !schema.object_window.categories.contains_key(*s))
        .map(String::as_str)
        .collect();
    missing.sort_unstable();
    assert!(
        missing.is_empty(),
        "record types with no object-window entry: {missing:?}"
    );

    let mut wrong: Vec<String> = schema
        .object_window
        .categories
        .iter()
        .filter(|(_, c)| !c.hidden)
        .filter_map(|(sig, c)| match c.editor.as_deref() {
            None => Some(format!("{sig} is shown but names no editor block")),
            Some(name) if nvre_editor::object_editor::editor_block(name).is_none() => Some(
                format!("{sig} names editor block {name:?}, which is not registered"),
            ),
            Some(_) => None,
        })
        .collect();
    wrong.sort();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
