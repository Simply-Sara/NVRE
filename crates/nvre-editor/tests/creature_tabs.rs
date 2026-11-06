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
///
/// Thirteen tabs in GECK; the editor adds **General** in front of them, which is
/// where GECK puts the identity and the flags a modder reaches for first.
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

/// The strip the editor draws: GECK's thirteen, with General in front.
const EDITOR_TABS: [&str; 14] = [
    "General",
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
    // Stats and AI Data because that is where it has always been. General is the
    // one heading that is not GECK's, and it goes first.
    for id in ["fnv", "fo3"] {
        let schema = GameProfile::by_id(id)
            .unwrap_or_else(|| panic!("{id}"))
            .load_schema()
            .expect("the schema loads");
        let tabs = schema.page_tabs("CREA");
        assert_eq!(
            tabs,
            EDITOR_TABS.to_vec(),
            "{id}: the creature page's tab strip"
        );
        assert_eq!(
            &tabs[1..],
            &GECK_TABS[..],
            "{id}: GECK's own tabs, in GECK's order, behind General"
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
            assert!(!drawn.is_empty(), "{id}: the {tab:?} tab draws no section");
        }
    }
}

#[test]
fn the_blood_tab_is_gecks_two_boxes_in_both_games() {
    // GECK's creature Blood tab is two group boxes — "When we are hit" (the
    // material the engine reports when this creature is struck, plus the two
    // blood flags) and "When we hit something else (melee)" (the impact data its
    // own attacks apply). Fallout 3 and New Vegas share the creature layout, so
    // both pages must say the same thing; a change applied to one directory and
    // not the other is the drift this test exists for.
    for id in ["fnv", "fo3"] {
        let schema = GameProfile::by_id(id)
            .unwrap_or_else(|| panic!("{id}"))
            .load_schema()
            .expect("the schema loads");
        let sections = schema.page_sections("CREA");
        let blood = sections_on_tab(&sections, Some("Blood"));
        let titles: Vec<&str> = blood.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            ["When we are hit", "When we hit something else (melee)"],
            "{id}: the Blood tab's two boxes"
        );
        assert_eq!(
            blood[0].subs,
            ["NAM4", "ACBS"],
            "{id}: the hit box shows the material type and the flags"
        );
        assert_eq!(
            blood[0].flags_bits.get("ACBS.flags").map(Vec::as_slice),
            Some(&[11u32, 12][..]),
            "{id}: the hit box shows the two blood flags"
        );
        assert_eq!(
            blood[1].subs,
            ["CNAM"],
            "{id}: the melee box is the impact data set alone"
        );
    }
}

#[test]
fn the_destruction_tab_is_gecks_checkbox_and_the_stage_list_in_both_games() {
    // GECK's creature Destruction page (DIALOGEX 0xc4a24c) is one
    // `Destructible Object` checkbox. The record carries a header and a stage
    // list, and Fallout 3 and New Vegas share the creature layout, so both
    // pages must name all five subrecords and hand them to the destruction
    // widget — otherwise `DSTD` falls into the unclaimed-subrecord box.
    for id in ["fnv", "fo3"] {
        let schema = GameProfile::by_id(id)
            .unwrap_or_else(|| panic!("{id}"))
            .load_schema()
            .expect("the schema loads");
        let sections = schema.page_sections("CREA");
        let destruction = sections_on_tab(&sections, Some("Destruction"));
        assert_eq!(destruction.len(), 1, "{id}: the Destruction tab's page");
        let page = destruction[0];
        assert_eq!(
            page.subs,
            ["DEST", "DSTD", "DMDL", "DMDT", "DSTF"],
            "{id}: the page names the header, the stage and its terminator"
        );
        assert_eq!(
            page.widget.as_deref(),
            Some("destruction"),
            "{id}: the page is drawn by the destruction widget"
        );
        assert!(
            page.untitled,
            "{id}: the widget draws GECK's own two captions"
        );
        // The header and the stage data resolve for the record, which is what
        // lets the widget read them without a record-specific definition.
        for sub in ["DEST", "DSTD", "DSTF"] {
            let sig: [u8; 4] = sub.as_bytes().try_into().expect("four bytes");
            assert!(
                schema.lookup(b"CREA", &sig).is_some(),
                "{id}: {sub} does not resolve on CREA"
            );
        }
    }
}

#[test]
fn the_sounds_tab_is_one_list_in_both_games() {
    // GECK's Sounds tab is two bare combos over a "Creature Sound Data" box, and
    // the box holds one row per sound. Fallout 3 and New Vegas share the
    // creature layout, so both pages must say the same thing.
    for id in ["fnv", "fo3"] {
        let schema = GameProfile::by_id(id)
            .unwrap_or_else(|| panic!("{id}"))
            .load_schema()
            .expect("the schema loads");
        let sections = schema.page_sections("CREA");
        let sounds = sections_on_tab(&sections, Some("Sounds"));
        let titles: Vec<&str> = sounds.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Sounds", "Creature Sound Data"],
            "{id}: the Sounds tab's two parts"
        );
        assert!(
            sounds[0].untitled,
            "{id}: Sound Level and Inherits Sounds From sit bare"
        );
        assert_eq!(sounds[0].subs, ["NAM5", "CSCR"], "{id}: the two combos");
        assert_eq!(
            sounds[1].subs,
            ["CSDT", "CSDI", "CSDC"],
            "{id}: the sound box names the whole triple"
        );
        assert_eq!(
            sounds[1].widget.as_deref(),
            Some("sounds"),
            "{id}: the box is drawn by the sounds widget"
        );
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
    let titles =
        |v: &[&nvre_esm::schema::SectionDef]| v.iter().map(|s| s.title.clone()).collect::<Vec<_>>();
    assert!(
        traits.iter().any(|s| s.title == "Traits"),
        "Traits draws its own flat Traits section: {:?}",
        titles(&traits)
    );
    assert!(
        !traits.iter().any(|s| s.title == "Flags"),
        "Flags moved to General and must not linger on Traits: {:?}",
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
                    // A flags word is one leaf, so two tabs narrowing it is the
                    // split itself — `ACBS.flags` on General and on Traits is how
                    // the bits are divided, and the bit-level rule is what keeps
                    // that honest. It still counts as named: the word is on the
                    // page, and its bits are checked one level down.
                    if flag_bits_of(&schema.resolve_def(&def_of(&schema, sub).ty), p).is_some() {
                        named.insert(p.as_str());
                        continue;
                    }
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
            let mut missing: Vec<&String> =
                all.iter().filter(|p| !named.contains(p.as_str())).collect();
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

/// Every bit of every split flags word is on exactly one tab.
///
/// The rule that replaces "a narrowed field is on one section" for a flags word,
/// because a flags word is one leaf and can only be split by its **bits**. Two
/// tabs may both narrow `ACBS.flags` — that is the split — but a bit named by
/// both would be a checkbox on two tabs, and a bit named by neither would be
/// uneditable everywhere with no "Everything else" band to catch it: the
/// subrecord *is* named, so the band never fires.
#[test]
fn every_bit_of_a_split_flags_word_is_on_exactly_one_tab() {
    for id in ["fnv", "fo3"] {
        let schema = GameProfile::by_id(id)
            .unwrap_or_else(|| panic!("{id}"))
            .load_schema()
            .expect("the schema loads");
        let sections = schema.page_sections("CREA");
        let record = schema.records.get("CREA").expect("CREA is in the schema");
        // `subrecord.field` -> (bit, the tabs that name it).
        let mut seen: std::collections::BTreeMap<String, Vec<(u32, String)>> =
            std::collections::BTreeMap::new();
        for section in &sections {
            for (key, bits) in &section.flags_bits {
                for bit in bits {
                    seen.entry(key.clone()).or_default().push((
                        *bit,
                        section
                            .tab
                            .clone()
                            .unwrap_or_else(|| "(every tab)".to_string()),
                    ));
                }
            }
        }
        assert!(
            !seen.is_empty(),
            "{id}: the creature page splits no flags word, so this proves nothing"
        );
        for (key, hits) in &seen {
            let (sub, path) = key.split_once('.').expect("a subrecord and a field");
            let def = record
                .subrecords
                .get(sub)
                .or_else(|| schema.common_subrecords.get(sub))
                .unwrap_or_else(|| panic!("{id}: {sub}"));
            let resolved = schema.resolve_def(&def.ty);
            let all = flag_bits_of(&resolved, path)
                .unwrap_or_else(|| panic!("{id}: {key} is not a flags field"));
            let named: std::collections::BTreeSet<u32> = hits.iter().map(|(bit, _)| *bit).collect();
            let missing: Vec<u32> = all
                .iter()
                .map(|(bit, _)| *bit)
                .filter(|bit| !named.contains(bit))
                .collect();
            assert!(
                missing.is_empty(),
                "{id}: {key} is split across tabs but bits {missing:?} are on none of \
                 them — those checkboxes would be uneditable everywhere"
            );
            // And no bit on two tabs, which would be the same checkbox twice.
            //
            // There is no exemption. There was one: the schema used to name
            // bit 27 twice — "Not Pushable / Allow Pickpocket" — after xEdit's
            // wbDefinitionsFNV list, which put the same checkbox on two tabs by
            // accident. fopdoc settles it: 27 Not Pushable, 28 Allow Pickpocket,
            // 29 Is Ghost, 30 No Rotating To Head-track, 31 Invulnerable. The
            // old double-name had also left every later name one bit too low,
            // so 28 read Is Ghost, 29 No Rotating To Head-track and 30
            // Invulnerable. The schema now follows fopdoc.
            let mut dupes: Vec<u32> = Vec::new();
            for bit in &named {
                let tabs: Vec<&String> = hits
                    .iter()
                    .filter(|(b, _)| b == bit)
                    .map(|(_, t)| t)
                    .collect();
                if tabs.len() > 1 {
                    dupes.push(*bit);
                }
            }
            assert!(
                dupes.is_empty(),
                "{id}: {key} puts bits {dupes:?} on more than one tab"
            );
        }
    }
}

/// A subrecord's definition, from the record's own map or the shared one.
fn def_of<'a>(schema: &'a nvre_esm::Schema, sub: &str) -> &'a nvre_esm::schema::SubDef {
    schema
        .records
        .get("CREA")
        .and_then(|r| r.subrecords.get(sub))
        .or_else(|| schema.common_subrecords.get(sub))
        .unwrap_or_else(|| panic!("{sub} is not a subrecord of CREA"))
}

/// The named bits of the flags field at `path`, as `(bit, name)`.
fn flag_bits_of(resolved: &nvre_esm::TypeDef, path: &str) -> Option<Vec<(u32, String)>> {
    fn at(t: &nvre_esm::TypeDef, parts: &[&str]) -> Option<nvre_esm::TypeDef> {
        let Some((part, rest)) = parts.split_first() else {
            return Some(t.clone());
        };
        match t {
            nvre_esm::TypeDef::Struct(fields) => {
                let f = fields.iter().find(|f| f.name == *part)?;
                at(&f.ty, rest)
            }
            nvre_esm::TypeDef::Union(variants) => {
                for (_, v) in variants {
                    if let Some(found) = at(v, parts) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }
    let parts: Vec<&str> = if path.is_empty() {
        Vec::new()
    } else {
        path.split('.').collect()
    };
    match at(resolved, &parts)? {
        nvre_esm::TypeDef::Flags { bits, .. } => Some(bits),
        _ => None,
    }
}

/// Every dotted field path a type has, the way the grid addresses them.
fn field_paths(resolved: &nvre_esm::TypeDef) -> Vec<String> {
    fn walk(t: &nvre_esm::TypeDef, prefix: &str, out: &mut Vec<String>) {
        use nvre_esm::TypeDef;
        match t {
            TypeDef::Struct(fields) => {
                for f in fields {
                    // Binary padding fields (`wbUnused` in xEdit) are not authored
                    // game properties, so sections do not name them.
                    if f.optional || f.name == "unused" {
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
