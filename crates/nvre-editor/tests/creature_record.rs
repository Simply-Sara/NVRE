//! The creature editor against a real creature out of the game's master.
//!
//! The tabs are data, so the tests above prove the data is complete. These prove
//! the window uses it: a tab draws the fields it is about and not the rest, and a
//! field that lives on a split subrecord can still be edited — `ACBS` carries a
//! flags word on Traits and a level on Stats, and an edit to one must re-encode
//! the whole subrecord without disturbing the other.
//!
//! The fixture is `VStreetCrBrahmin2` (`0017B587`): a brahmin with a model list, a
//! texture-hash array, a faction list and a full `ACBS`, so one record exercises
//! most of the tabs. Its values are the ones `dump_record CREA` prints.
//!
//! Needs `NVRE_DATA`.

mod common;

use common::*;
use nvre_editor::object_editor::{self, EditorTab, ObjectEditorState, Pages};
use nvre_editor::panels::sections_on_tab;
use nvre_editor::session::Session;

/// The fixture's FormID.
const BRAHMIN: u32 = 0x0017_B587;

/// A session with the fixture open in the object editor.
fn open_creature() -> Option<(Session, ObjectEditorState)> {
    let mut session = open_master()?;
    let fid = session
        .records_of(*b"CREA")
        .iter()
        .find(|(fid, _)| *fid == BRAHMIN)
        .map(|(fid, _)| *fid)
        .or_else(|| session.records_of(*b"CREA").first().map(|(f, _)| *f))?;
    session.open_object(fid, *b"CREA");
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid, *b"CREA");
    Some((session, state))
}

#[test]
fn the_record_reads_as_the_game_wrote_it() {
    let Some((_session, state)) = open_creature() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    assert_eq!(state.edid, "VStreetCrBrahmin2", "the Editor ID");
    assert_eq!(state.full_name, "Brahmin", "the name");
    assert!(
        state.model.contains("Brahmin"),
        "the header's model is the record's MODL: {:?}",
        state.model
    );
}

#[test]
fn the_sections_split_across_the_tabs_and_none_is_lost() {
    // Every tab must be reachable, and between them they must name every
    // subrecord the creature carries — including `NIFZ`/`NIFT` (its model list),
    // `SNAM` (factions) and `SPLO` (actor effects), which a creature's dialog is
    // the only place to edit.
    let Some((session, _)) = open_creature() else {
        return;
    };
    let schema = &session.schema;
    let sections = schema.page_sections("CREA");
    let tabs = schema.page_tabs("CREA");
    let mut named: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for tab in &tabs {
        let drawn = sections_on_tab(&sections, Some(tab));
        assert!(!drawn.is_empty(), "the {tab} tab draws nothing");
        for s in drawn {
            named.extend(s.subs.iter().cloned());
        }
    }
    // The subrecords the fixture actually carries, read from the record rather
    // than from the schema, so this says what the game stores.
    let subs = subs(&session, BRAHMIN);
    let mut missing: Vec<String> = subs
        .iter()
        .map(|s| s.sig_str())
        .filter(|sig| !named.contains(sig) && !["EDID", "OBND", "FULL", "MODL"].contains(&sig.as_str()))
        .collect();
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "the creature's page names no tab for {missing:?}"
    );
}

#[test]
fn the_traits_tab_shows_the_creature_type_and_the_stats_tab_does_not() {
    // The two tabs whose split is the whole point: Traits carries the header
    // flags, Stats carries the level. `ACBS` is one subrecord with both, so a page
    // that could not narrow it would have to show the whole word on both tabs.
    let Some((session, mut state)) = open_creature() else {
        return;
    };
    let sections = session.schema.page_sections("CREA");
    let traits = sections_on_tab(&sections, Some("Traits"));
    let stats = sections_on_tab(&sections, Some("Stats"));

    let flags_section = traits
        .iter()
        .find(|s| s.tab.as_deref() == Some("Traits") && s.fields.contains_key("ACBS"))
        .expect("Traits narrows ACBS");
    assert!(
        flags_section.fields["ACBS"].contains(&"flags".to_string()),
        "Traits shows ACBS's flags: {:?}",
        flags_section.fields
    );
    assert!(
        !stats
            .iter()
            .any(|s| s.fields.get("ACBS").is_some_and(|f| f.contains(&"flags".to_string()))),
        "Stats must not show ACBS's flags as well"
    );

    // And the state survives a tab change, which is what makes the strip usable:
    // the modder's record does not reload when they look at another tab.
    state.tab.chosen = Some("Stats".to_string());
    assert_eq!(state.tab.heading(&session.schema.page_tabs("CREA")), Some("Stats"));
    state.tab.chosen = Some("Traits".to_string());
    assert_eq!(state.tab.heading(&session.schema.page_tabs("CREA")), Some("Traits"));
    assert_eq!(state.edid, "VStreetCrBrahmin2");
}

#[test]
fn a_tab_name_the_strip_lost_falls_back_to_the_first_tab() {
    // A schema edit that renames a tab must not leave the window drawing nothing.
    let tabs = vec!["Traits".to_string(), "Stats".to_string()];
    let mut tab = EditorTab {
        chosen: Some("Gone".to_string()),
    };
    assert_eq!(tab.heading(&tabs), Some("Traits"));
    tab.chosen = None;
    assert_eq!(tab.heading(&tabs), Some("Traits"));
    assert_eq!(tab.heading(&[]), None, "a page with no strip has no tab");
}

#[test]
fn editing_the_level_on_stats_keeps_the_flags_on_traits() {
    // The write path for a split subrecord. Both fields are in `ACBS`, so an edit
    // to the level re-encodes the whole subrecord — and must write back the flags
    // word it did not touch, or setting a level silently clears every flag.
    let Some((mut session, _state)) = open_creature() else {
        return;
    };
    let before = payload(&subs(&session, BRAHMIN), b"ACBS");
    let flags_before = u32::from_le_bytes([before[0], before[1], before[2], before[3]]);

    // The level field, as the Stats tab addresses it.
    session
        .set_object_field(BRAHMIN, b"ACBS", "level_or_level_mult_raw", serde_json::json!(7))
        .expect("the level is writable");

    let after = payload(&subs(&session, BRAHMIN), b"ACBS");
    let flags_after = u32::from_le_bytes([after[0], after[1], after[2], after[3]]);
    assert_eq!(
        flags_after, flags_before,
        "editing the level changed the flags word: {flags_before:08X} -> {flags_after:08X}"
    );
    assert_eq!(
        u16::from_le_bytes([after[8], after[9]]),
        7,
        "the level did not take: the subrecord is {after:02X?}"
    );
    assert_eq!(after.len(), before.len(), "the subrecord's length moved");
}

#[test]
fn a_faction_can_be_added_and_removed_from_its_tab() {
    // The Factions tab is a `repeat` widget, which is the widget for a subrecord a
    // record carries many of — the brahmin has three `SNAM`. The edit path is
    // `set_sub_list`, because `set_sub` holds one payload per signature and would
    // collapse the three onto the last.
    let Some((mut session, _state)) = open_creature() else {
        return;
    };
    let before = payloads(&subs(&session, BRAHMIN), b"SNAM");
    assert_eq!(before.len(), 3, "the fixture has three factions");

    let mut after = before.clone();
    after.pop();
    session.doc.set_sub_list(BRAHMIN, b"SNAM", after.clone());
    let read = payloads(&subs(&session, BRAHMIN), b"SNAM");
    assert_eq!(read.len(), 2, "removing one faction left {}", read.len());
    assert_eq!(read, after[..2], "the remaining factions are the first two");
}

#[test]
fn the_general_tab_holds_the_identity_and_the_flags_the_editor_puts_there() {
    // General is the first tab, and it is where the identity the header used to
    // hold now lives: the Editor ID, the name and the script. The header no
    // longer draws them, so this is the only place a modder can edit them.
    let Some((session, _)) = open_creature() else {
        return;
    };
    let schema = &session.schema;
    let sections = schema.page_sections("CREA");
    let general = sections_on_tab(&sections, Some("General"));
    let titles = |v: &[&nvre_esm::schema::SectionDef]| {
        v.iter().map(|s| s.title.clone()).collect::<Vec<_>>()
    };
    assert_eq!(
        schema.page_tabs("CREA").first().map(String::as_str),
        Some("General"),
        "General is the first tab"
    );
    assert!(
        general.iter().any(|s| s.title == "Identity"),
        "General draws Identity: {:?}",
        titles(&general)
    );
    // The identity is the whole subrecord, not a narrowed one — a name is one
    // field and there is nothing to split.
    let identity = general
        .iter()
        .find(|s| s.title == "Identity")
        .expect("Identity is on General");
    assert_eq!(
        identity.subs,
        vec!["EDID".to_string(), "FULL".to_string(), "SCRI".to_string()],
        "Identity holds the Editor ID, the name and the script"
    );
    // And the template data, with the ActorBase picker beside the checkboxes.
    let template = general
        .iter()
        .find(|s| s.title == "Template Data")
        .expect("Template Data is on General");
    assert!(
        template.subs.contains(&"TPLT".to_string()),
        "Template Data holds the ActorBase picker: {:?}",
        template.subs
    );
    assert_eq!(
        template.flags_bits.get("ACBS.template_flags").map(Vec::len),
        Some(10),
        "the ten template checkboxes, one per bit"
    );
}

#[test]
fn editing_a_flag_on_general_leaves_the_flags_on_traits_alone() {
    // The write path for a split flags word. `ACBS.flags` is narrowed on two
    // tabs, so a checkbox ticked on General re-encodes the whole subrecord — and
    // must write back the bits that belong to Traits, or setting Essential
    // silently clears Biped, Swims and everything else.
    let Some((mut session, _state)) = open_creature() else {
        return;
    };
    // The record as the editor sees it, which is the file's bytes with the
    // queued edits applied — an edit is written to the document, not to the
    // record in the file, so reading the raw record would see the old bytes.
    let read = |session: &nvre_editor::session::Session| -> Vec<u8> {
        let rec = session
            .resolved_record(BRAHMIN)
            .expect("the record resolves");
        rec.subrecords()
            .expect("subrecords parse")
            .iter()
            .find(|s| &s.sig == b"ACBS")
            .map(|s| s.data.clone())
            .expect("an ACBS subrecord")
    };
    let before = read(&session);
    let flags_before = u32::from_le_bytes([before[0], before[1], before[2], before[3]]);

    // Essential is bit 1 — one of the ten General owns. The value is the whole
    // word with that bit set, which is what the grid writes when a checkbox is
    // ticked on a split section.
    let with_essential = flags_before | (1 << 1);
    session
        .set_object_field(
            BRAHMIN,
            b"ACBS",
            "flags",
            serde_json::json!({
                "value": with_essential,
                "set": ["Essential"],
            }),
        )
        .expect("the flags word is writable");

    let after = read(&session);
    let flags_after = u32::from_le_bytes([after[0], after[1], after[2], after[3]]);
    assert_eq!(
        flags_after, with_essential,
        "the flags word after the edit: {flags_before:08X} -> {flags_after:08X}"
    );
    // Every bit that was on before is still on — the ones General does not show
    // included. That is the invariant the split exists not to break.
    assert_eq!(
        flags_after & flags_before,
        flags_before,
        "editing a flag on General cleared a bit that belongs to Traits"
    );
    assert_eq!(after.len(), before.len(), "the subrecord's length moved");
}

#[test]
fn the_general_tab_draws_a_checkbox_for_every_flag_it_names() {
    // The split is only real if the checkboxes exist. A flags field renders as
    // `(object)` — no checkboxes at all — when the grid cannot find its bits,
    // which is what happened to `ACBS.flags` for as long as `flag_bits` resolved
    // the subrecord's own type: `ACBS` is a struct, so the lookup answered
    // `Struct` and every flag on every tab was invisible.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    let tabs = session.schema.page_tabs("CREA");
    let schema_bits = |tab: &str, key: &str| -> Vec<u32> {
        session
            .schema
            .page_sections("CREA")
            .iter()
            .filter(|s| s.tab.as_deref() == Some(tab))
            .filter_map(|s| s.flags_bits.get(key).cloned())
            .next()
            .unwrap_or_default()
    };
    let expected_general = schema_bits("General", "ACBS.flags");
    let expected_traits = schema_bits("Traits", "ACBS.flags");
    assert_eq!(expected_general.len(), 10, "General names ten flags");
    assert_eq!(expected_traits.len(), 17, "Traits keeps the other seventeen");

    // The schema's own name for each bit, so the assertion is about the labels a
    // modder reads rather than about a count. Resolved before the loop, because
    // rendering needs the session mutably and the names come from its schema.
    let acbs = session
        .schema
        .records
        .get("CREA")
        .and_then(|r| r.subrecords.get("ACBS"))
        .expect("ACBS is a subrecord of CREA");
    let nvre_esm::TypeDef::Struct(acbs_fields) = session.schema.resolve_def(&acbs.ty) else {
        panic!("ACBS is a struct");
    };
    let nvre_esm::TypeDef::Flags { bits, .. } = &acbs_fields
        .iter()
        .find(|f| f.name == "flags")
        .expect("ACBS.flags")
        .ty
    else {
        panic!("ACBS.flags is a flags word");
    };
    let named: std::collections::BTreeMap<u32, String> = bits
        .iter()
        .map(|(bit, name)| (*bit, name.clone()))
        .collect();

    for (tab, expected) in [("General", &expected_general), ("Traits", &expected_traits)] {
        state.tab.chosen = Some(tab.to_string());
        assert_eq!(state.tab.heading(&tabs), Some(tab));
        let names = checkbox_labels(&mut session, &mut state);
        // Every bit the section names must be a checkbox the tab draws, under
        // the schema's own name for it.
        let missing: Vec<String> = expected
            .iter()
            .filter_map(|bit| named.get(bit).cloned())
            .filter(|name| !names.iter().any(|n| n == name))
            .collect();
        assert!(
            missing.is_empty(),
            "{tab} draws no checkbox for {missing:?}. A flags field whose bits the \\
             grid cannot find renders as `(object)` — no checkboxes at all — so \\
             the flags are invisible and uneditable on this tab"
        );
        // And no flag the section does *not* name leaks onto this tab, which is
        // the other half of the split: the same checkbox on two tabs is not a
        // split, it is a duplicate.
        let other = if tab == "General" {
            &expected_traits
        } else {
            &expected_general
        };
        let leaked: Vec<String> = other
            .iter()
            .filter_map(|bit| named.get(bit).cloned())
            .filter(|name| names.iter().any(|n| n == name))
            .collect();
        assert!(
            leaked.is_empty(),
            "{tab} draws checkboxes for flags it does not own: {leaked:?}"
        );
    }
}

/// The labels of every checkbox the creature editor draws, via egui's
/// accessibility tree.
///
/// The tree is the only way to see what a widget *rendered*: the schema says
/// which bits a section names, and the grid is what decides whether a checkbox
/// exists for them. A flags field with no bits renders `(object)` — no checkbox
/// at all — and the schema's answer would not say so.
fn checkbox_labels(session: &mut Session, state: &mut ObjectEditorState) -> Vec<String> {
    use nvre_editor::egui;
    let mut pages = Pages::default();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let size = egui::vec2(900.0, 2400.0);
    let mut labels = Vec::new();
    for _ in 0..3 {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    object_editor::body(ui, session, state, &mut pages);
                });
            },
        );
        out.textures_delta.clear();
        labels = out
            .platform_output
            .accesskit_update
            .map(|u| {
                u.nodes
                    .into_iter()
                    .filter_map(|(_, node)| node.label().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
    }
    labels
}
