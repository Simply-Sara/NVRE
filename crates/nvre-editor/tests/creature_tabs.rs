//! The tabbed page: a record's fields split across the tabs its game's editor
//! shows, and the two rules that keep the split honest.
//!
//! A tabbed page is the same described page as any other, with two additions. A
//! section can say which tab it belongs to, and a section can narrow one
//! subrecord to some of its fields — because the game's own editor does exactly
//! that, putting `ACBS`'s flags on Traits and its leveling on Stats.
//!
//! Splitting a subrecord is where a page can lose data without saying so: a
//! `fields` list that misses a field hides it on every tab, and unlike a missing
//! section there is no "Everything else" band to catch it (the subrecord *is*
//! named, so the band does not fire). So the tests here are about coverage:
//! every field of a narrowed subrecord is named by some section of the same page.

use nvre_editor::game::GameProfile;
use nvre_editor::panels::sections_on_tab;

/// GECK's creature dialog, from geckwiki.com's Category:Creatures.
const GECK_TABS: [&str; 13] = [
    "Traits",
    "Stats",
    "Factions",
    "AI Data",
    "AI Packages",
    "Inventory",
    "Actor Effect List",
    "Animation",
    "ModelList",
    "Blood",
    "Sounds",
    "Destruction",
    "Scripts",
];

#[test]
fn the_creature_page_has_gecks_tabs_in_gecks_order() {
    // The order is part of the answer: a modder looking for Factions looks between
    // Stats and AI Data because that is where it has always been.
    for id in ["fnv", "fo3"] {
        let schema = GameProfile::by_id(id)
            .unwrap_or_else(|| panic!("{id}"))
            .load_schema()
            .expect("the schema loads");
        assert_eq!(
            schema.page_tabs("CREA"),
            GECK_TABS.to_vec(),
            "{id}: the creature page's tab strip"
        );
    }
}

#[test]
fn every_tab_on_the_strip_carries_at_least_one_section() {
    // A tab that draws nothing is a button that does nothing. Every heading on the
    // strip must be some section's tab, or the strip is advertising a page that
    // does not exist.
    for id in ["fnv", "fo3"] {
        let schema = GameProfile::by_id(id)
            .unwrap_or_else(|| panic!("{id}"))
            .load_schema()
            .expect("the schema loads");
        let sections = schema.page_sections("CREA");
        for tab in schema.page_tabs("CREA") {
            let drawn = sections_on_tab(&sections, Some(&tab));
            assert!(
                !drawn.is_empty(),
                "{id}: the {tab:?} tab draws no section"
            );
        }
    }
}

#[test]
fn a_section_on_another_tab_is_not_drawn() {
    // The mechanism. A section filed on Stats must not appear while Traits is
    // chosen, or the tabs are decorative and every tab shows the whole record.
    let schema = GameProfile::by_id("fnv")
        .expect("fnv")
        .load_schema()
        .expect("the schema loads");
    let sections = schema.page_sections("CREA");
    let traits = sections_on_tab(&sections, Some("Traits"));
    let stats = sections_on_tab(&sections, Some("Stats"));
    let titles = |v: &[&nvre_esm::schema::SectionDef]| {
        v.iter().map(|s| s.title.clone()).collect::<Vec<_>>()
    };
    assert!(
        traits.iter().any(|s| s.title == "Flags"),
        "Traits draws its own Flags section: {:?}",
        titles(&traits)
    );
    assert!(
        !traits.iter().any(|s| s.title == "Leveling"),
        "Leveling belongs to Stats and must not be on Traits: {:?}",
        titles(&traits)
    );
    assert!(
        stats.iter().any(|s| s.title == "Leveling"),
        "Stats draws Leveling: {:?}",
        titles(&stats)
    );
    assert!(
        !stats.iter().any(|s| s.title == "Factions"),
        "Factions has a tab of its own: {:?}",
        titles(&stats)
    );
}

#[test]
fn a_section_with_no_tab_is_drawn_on_every_one() {
    // A page can carry a section that is the record's shared part rather than any
    // tab's. It has to be reachable wherever the modder is, or it is a section
    // nobody can find.
    let mut schema = nvre_esm::Schema::new();
    schema
        .merge_str(
            r#"{
              "format": "fnv.schema/1",
              "object_window": {"categories": {"TEST": {
                "group": "Miscellaneous",
                "tabs": ["One", "Two"],
                "sections": [
                  {"title": "Only One", "tab": "One", "subs": ["AAAA"]},
                  {"title": "Only Two", "tab": "Two", "subs": ["BBBB"]},
                  {"title": "Always", "subs": ["CCCC"]}
                ]
              }}}
            }"#,
        )
        .expect("the schema loads");
    let sections = schema.page_sections("TEST");
    for tab in ["One", "Two"] {
        let drawn = sections_on_tab(&sections, Some(tab));
        assert!(
            drawn.iter().any(|s| s.title == "Always"),
            "{tab} does not draw the shared section"
        );
        assert_eq!(drawn.len(), 2, "{tab}: its own section plus the shared one");
    }
    // And a page with no strip at all draws every section, which is every page
    // that is not tabbed.
    assert_eq!(sections_on_tab(&sections, None).len(), 3);
}

#[test]
fn a_section_filed_on_a_tab_the_page_does_not_have_is_a_schema_error() {
    // The typo this is for: a section filed on "Faction" when the strip says
    // "Factions". Nothing would draw it, and there is no fallback band for a
    // section, so the check has to happen at parse time.
    let mut schema = nvre_esm::Schema::new();
    schema
        .merge_str(
            r#"{
              "format": "fnv.schema/1",
              "object_window": {"categories": {"TEST": {
                "group": "Miscellaneous",
                "tabs": ["Factions"],
                "sections": [{"title": "F", "tab": "Faction", "subs": ["AAAA"]}]
              }}}
            }"#,
        )
        .expect("the schema loads");
    let err = schema.validate().expect_err("the typo is refused");
    assert!(
        err.contains("Faction") && err.contains("Factions"),
        "the message names the tab and the strip: {err}"
    );
}

#[test]
fn a_narrowing_field_path_is_read_from_the_schema() {
    // The shape, through the layer chain, so a game that restates a shared heading
    // to narrow it gets the narrowing rather than the template's whole list.
    let mut schema = nvre_esm::Schema::new();
    schema
        .merge_str(
            r#"{
              "format": "fnv.schema/1",
              "pages": {"tpl": {
                "tabs": ["A", "B"],
                "sections": [{"title": "Split", "tab": "A", "subs": ["AAAA"]}]
              }},
              "object_window": {"categories": {"TEST": {
                "group": "Miscellaneous",
                "extends": "tpl",
                "sections": [
                  {"title": "Split", "tab": "A", "subs": ["AAAA"],
                   "fields": {"AAAA": ["one", "two"]}},
                  {"title": "Second", "tab": "B", "subs": ["AAAA"],
                   "fields": {"AAAA": ["three"]}}
                ]
              }}}
            }"#,
        )
        .expect("the schema loads");
    let sections = schema.page_sections("TEST");
    let split = sections
        .iter()
        .find(|s| s.title == "Split")
        .expect("the shared heading merged rather than repeating");
    assert_eq!(
        split.fields.get("AAAA").map(Vec::as_slice),
        Some(&["one".to_string(), "two".to_string()][..]),
        "the game's narrowing wins over the template's whole-subrecord section"
    );
    assert_eq!(
        schema.page_tabs("TEST"),
        vec!["A".to_string(), "B".to_string()],
        "the template's strip is inherited"
    );
}

#[test]
fn every_field_of_a_narrowed_subrecord_is_named_somewhere_on_the_page() {
    // The rule that keeps a split honest. `ACBS` is narrowed across Traits and
    // Stats; a field neither section names would be uneditable on every tab, and
    // the "Everything else" band would not catch it because `ACBS` *is* named.
    let mut checked = 0;
    for id in ["fnv", "fo3"] {
        let schema = GameProfile::by_id(id)
            .unwrap_or_else(|| panic!("{id}"))
            .load_schema()
            .expect("the schema loads");
        let sections = schema.page_sections("CREA");
        let record = schema.records.get("CREA").expect("CREA is in the schema");
        let mut narrowed: std::collections::BTreeMap<&str, std::collections::BTreeSet<&str>> =
            std::collections::BTreeMap::new();
        for section in &sections {
            for (sub, paths) in &section.fields {
                assert!(
                    record.subrecords.contains_key(sub)
                        || schema.common_subrecords.contains_key(sub),
                    "{id}: {sub} is narrowed but is not a subrecord of CREA"
                );
                let named = narrowed.entry(sub.as_str()).or_default();
                for p in paths {
                    assert!(
                        named.insert(p.as_str()),
                        "{id}: {sub}.{p} is named by more than one section on the same page"
                    );
                }
            }
        }
        for (sub, named) in &narrowed {
            let def = record
                .subrecords
                .get(*sub)
                .or_else(|| schema.common_subrecords.get(*sub))
                .unwrap_or_else(|| panic!("{id}: {sub}"));
            let resolved = schema.resolve_def(&def.ty);
            let all = field_paths(&resolved);
            let mut missing: Vec<&String> = all
                .iter()
                .filter(|p| !named.contains(p.as_str()))
                .collect();
            missing.sort();
            assert!(
                missing.is_empty(),
                "{id}: {sub} is split across tabs but {missing:?} is on none of them — \
                 those fields would be uneditable everywhere"
            );
            checked += named.len();
        }
    }
    assert!(
        checked > 0,
        "no page narrows a subrecord, so this proves nothing"
    );
}

/// Every dotted field path a type has, the way the grid addresses them.
fn field_paths(resolved: &nvre_esm::TypeDef) -> Vec<String> {
    fn walk(t: &nvre_esm::TypeDef, prefix: &str, out: &mut Vec<String>) {
        use nvre_esm::TypeDef;
        match t {
            TypeDef::Struct(fields) => {
                for f in fields {
                    if f.optional {
                        continue;
                    }
                    let path = if prefix.is_empty() {
                        f.name.clone()
                    } else {
                        format!("{prefix}.{}", f.name)
                    };
                    walk(&f.ty, &path, out);
                }
            }
            TypeDef::Union(variants) => {
                // The grid shows the variant the record chose; every variant's
                // fields are reachable through the same path space, so all of them
                // are counted.
                for (_, v) in variants {
                    walk(v, prefix, out);
                }
            }
            // A leaf: a primitive, an enum, a flags word, a FormID. The grid edits
            // it as one value at this path.
            _ => {
                if !prefix.is_empty() && !out.iter().any(|p| p == prefix) {
                    out.push(prefix.to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(resolved, "", &mut out);
    out
}
