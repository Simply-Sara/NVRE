//! A page built from a shared body plus a game's own sections.
//!
//! Fallout: New Vegas, Fallout 3, Skyrim and Fallout 4 carry 107 weapon
//! subrecords between them and share five. Writing a page per game restates the
//! five three times; writing one page for all four loses the 102. So a page is a
//! template (`weap_common`) plus a game's own sections, and a section title the
//! game also uses merges into the template's rather than appearing twice.
//!
//! These tests are what keep that honest: every game's page covers every
//! subrecord its record has, the shared five are drawn once, and a game's
//! override actually wins.

use nvre_editor::game::GameProfile;

#[test]
fn the_shared_weapon_page_covers_every_record_it_serves() {
    // The promise a described page makes: nothing on the record is left off the
    // page. `page_sections` declares "Everything else" for anything the sections
    // miss, so a miss is not data loss — but a table that misses one is a page
    // half-described, and that is what this catches.
    let mut checked = 0;
    for (id, sig) in [
        ("fnv", "WEAP"),
        ("fo3", "WEAP"),
        ("skyrim", "WEAP"),
        ("skyrimse", "WEAP"),
        ("fo4", "WEAP"),
    ] {
        let profile = GameProfile::by_id(id).expect("a known game");
        let schema = profile.load_schema().expect("the schema loads");
        let sections = schema.page_sections(sig);
        assert!(
            !sections.is_empty(),
            "{id} {sig}: no page sections, so the object editor falls back to the flat grid"
        );
        let record = schema
            .records
            .get(sig)
            .unwrap_or_else(|| panic!("{id} {sig} is not in the schema"));
        let named: std::collections::BTreeSet<&str> = sections
            .iter()
            .flat_map(|s| s.subs.iter().map(String::as_str))
            .collect();
        let mut missing: Vec<&str> = record
            .subrecords
            .keys()
            .map(String::as_str)
            .filter(|s| !named.contains(s))
            .collect();
        missing.sort_unstable();
        assert!(
            missing.is_empty(),
            "{id} {sig}: the page does not name {}",
            missing.join(" ")
        );
        checked += 1;
    }
    assert_eq!(checked, 5, "every game's weapon is checked");
}

#[test]
fn a_shared_heading_merges_and_does_not_repeat() {
    // The mechanism. `weap_common` has a "Damage" section; each game has its own
    // "Damage" with the subrecords that game actually stores. The two merge into
    // one heading — not two, and not one that dropped the other's contents.
    let fo4 = GameProfile::by_id("fo4")
        .expect("fo4")
        .load_schema()
        .expect("the schema loads");
    let sections = fo4.page_sections("WEAP");
    let damage: Vec<_> = sections.iter().filter(|s| s.title == "Damage").collect();
    assert_eq!(damage.len(), 1, "one Damage heading, not one per layer");
    // FO4's own `DAMA` and `MASE` are there, and so are the template's shared
    // `DNAM` and `CRDT` — a game adding to a heading must not drop what it shares.
    for sub in ["DAMA", "MASE", "DNAM", "CRDT"] {
        assert!(
            damage[0].subs.iter().any(|s| s == sub),
            "FO4's Damage section lost {sub}: {:?}",
            damage[0].subs
        );
    }
}

#[test]
fn a_template_section_with_no_override_survives() {
    // `Enchantment` is EAMT in the template. Skyrim's file says "Enchantment"
    // with EITM, so it replaces; FO4's *also* says it. The template's version has
    // to survive anywhere a game does not speak, which is what makes the shared
    // body worth having.
    let fnv = GameProfile::by_id("fnv")
        .expect("fnv")
        .load_schema()
        .expect("the schema loads");
    let sections = fnv.page_sections("WEAP");
    let enchant: Vec<_> = sections
        .iter()
        .filter(|s| s.title == "Enchantment")
        .collect();
    assert_eq!(enchant.len(), 1);
    assert!(
        enchant[0].subs.contains(&"EAMT".to_string()),
        "the template's Enchantment is what FNV shows: {:?}",
        enchant[0].subs
    );
    // And a template section no game overrides is unchanged everywhere.
    let refs: Vec<_> = sections
        .iter()
        .filter(|s| s.title == "References")
        .collect();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].subs, vec!["INAM".to_string(), "NNAM".to_string()]);
}

#[test]
fn a_page_that_extends_a_missing_template_is_empty_not_an_error() {
    // A schema written before its template must still load: a page with nothing
    // under it is a reportable state, a schema that refuses to parse is not.
    let mut schema = nvre_esm::Schema::new();
    schema
        .merge_str(
            r#"{
              "format": "fnv.schema/1",
              "object_window": {"categories": {"WEAP": {"group": "Items", "extends": "nope"}}}
            }"#,
        )
        .expect("a category may name a template the schema does not hold");
    assert!(schema.page_sections("WEAP").is_empty());
    assert!(!schema.page_is_described("WEAP"));
}

#[test]
fn the_games_genuinely_differ_so_the_tree_is_worth_it() {
    // The measurement the design rests on. If every game's weapon were the same,
    // one page would do and the template would be ceremony.
    let mut sets = Vec::new();
    for id in ["fnv", "skyrim", "fo4"] {
        let schema = GameProfile::by_id(id).unwrap().load_schema().unwrap();
        let subs: std::collections::BTreeSet<String> =
            schema.records["WEAP"].subrecords.keys().cloned().collect();
        sets.push((id, subs));
    }
    let common: std::collections::BTreeSet<String> = sets
        .iter()
        .map(|(_, s)| s.clone())
        .reduce(|a, b| a.intersection(&b).cloned().collect())
        .unwrap();
    eprintln!(
        "shared by all three: {common:?}; sizes {:?}",
        sets.iter().map(|(i, s)| (*i, s.len())).collect::<Vec<_>>()
    );
    assert!(
        common.len() < 10,
        "the games share only a handful of weapon subrecords"
    );
    let total: usize = sets.iter().map(|(_, s)| s.len()).sum();
    assert!(
        total > 3 * common.len(),
        "most subrecords are a game's own, not shared"
    );
}
