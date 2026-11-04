//! A page described by its schema sections, and the rule that keeps it honest.
//!
//! A section list is data, so it can be wrong: a game's table can name a
//! subrecord its records do not have (harmless — the section is simply absent)
//! or forget one its records do have. The second is the dangerous one: a page
//! that does not name a subrecord leaves it off the page, so part of the record
//! becomes uneditable with nothing saying so.
//!
//! Two things guard against that. The renderer shows whatever the sections did
//! not name under "Everything else", so nothing is ever unreachable; and these
//! tests assert the data itself is complete, so the fallback stays empty rather
//! than becoming the way every record is edited.

use nvre_editor::game::GameProfile;

const GAMES: [&str; 5] = ["fnv", "fo3", "skyrim", "skyrimse", "fo4"];

#[test]
fn every_sectioned_page_names_every_subrecord_its_record_has() {
    // A sectioned page that misses a subrecord still shows it (under "Everything
    // else"), but a table that misses one is a page half-described: the heading
    // a modder looks under is not there. Catching it here means the fallback is
    // for the odd case, not the normal one.
    let mut checked = 0;
    for id in GAMES {
        let profile = GameProfile::by_id(id).expect("a known game");
        let schema = profile.load_schema().expect("the schema loads");
        for (sig, cat) in &schema.object_window.categories {
            if cat.sections.is_empty() {
                continue;
            }
            checked += 1;
            let Some(record) = schema.records.get(sig) else {
                continue;
            };
            let named: std::collections::BTreeSet<&str> = cat
                .sections
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
                "{id} {sig}: the sections do not name {} — they would show under \
                 \"Everything else\" instead of their own heading",
                missing.join(" ")
            );
        }
    }
    assert!(
        checked > 0,
        "no game describes a page with sections, so this test proves nothing"
    );
    eprintln!("{checked} sectioned pages name every subrecord they describe");
}

#[test]
fn a_section_may_name_a_subrecord_only_another_game_has() {
    // The mechanism that lets one list serve several games: a section naming a
    // subrecord this record does not carry is skipped, not an error. Fallout 4's
    // weapon has no `VATS`; Fallout: New Vegas's does.
    let fnv = GameProfile::by_id("fnv")
        .expect("fnv")
        .load_schema()
        .unwrap();
    let fo4 = GameProfile::by_id("fo4")
        .expect("fo4")
        .load_schema()
        .unwrap();
    let fnv_weap = &fnv.records.get("WEAP").expect("fnv WEAP").subrecords;
    let fo4_weap = &fo4.records.get("WEAP").expect("fo4 WEAP").subrecords;
    assert!(
        fnv_weap.contains_key("VATS"),
        "FNV's weapon has a VATS block"
    );
    assert!(
        !fo4_weap.contains_key("VATS"),
        "FO4's weapon has no VATS block, so a VATS section is absent there"
    );
}

#[test]
fn a_section_signature_is_four_bytes() {
    // A section names a subrecord, and a subrecord's signature is four bytes. A
    // three- or five-character name matches nothing and would silently vanish
    // from the page — the schema refuses it at parse time, and this proves the
    // refusal reaches every table.
    for id in GAMES {
        let profile = GameProfile::by_id(id).expect("a known game");
        let schema = profile.load_schema().expect("the schema loads");
        for (sig, cat) in &schema.object_window.categories {
            for (i, section) in cat.sections.iter().enumerate() {
                assert!(
                    !section.title.trim().is_empty(),
                    "{id} {sig}: section {i} has no heading"
                );
                for sub in &section.subs {
                    assert_eq!(sub.len(), 4, "{id} {sig}: section {i} names {sub:?}");
                }
            }
        }
    }
}

#[test]
fn the_fallout_4_weapon_page_is_data_and_covers_the_record() {
    // The first page written as data rather than as code: Fallout 4's weapon had
    // no page at all, and now has one without a line of Rust for it.
    let fo4 = GameProfile::by_id("fo4")
        .expect("fo4")
        .load_schema()
        .unwrap();
    let cat = fo4
        .object_window
        .categories
        .get("WEAP")
        .expect("FO4 files WEAP in its object window");
    assert!(
        !cat.sections.is_empty(),
        "FO4's weapon page is described by its sections"
    );
    let named: std::collections::BTreeSet<&str> = cat
        .sections
        .iter()
        .flat_map(|s| s.subs.iter().map(String::as_str))
        .collect();
    let defined: std::collections::BTreeSet<&str> = fo4
        .records
        .get("WEAP")
        .expect("FO4 WEAP")
        .subrecords
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        named, defined,
        "the page names exactly the subrecords the record has"
    );
}
