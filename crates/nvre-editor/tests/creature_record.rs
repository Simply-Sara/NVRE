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
        vec!["EDID".to_string(), "FULL".to_string()],
        "Identity holds the Editor ID and the name"
    );
    // The script is on the Scripts tab, where GECK puts it. It used to sit on
    // General beside the name, which made the identity section look like the
    // record's whole header rather than its name.
    let scripts = sections_on_tab(&sections, Some("Scripts"));
    assert!(
        scripts
            .iter()
            .any(|s| s.subs.contains(&"SCRI".to_string())),
        "SCRI is not on the Scripts tab: {:?}",
        titles(&scripts)
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

/// The text of every **label** the creature editor draws, via egui's
/// accessibility tree.
///
/// The tree is the only way to see what a page *rendered*: the schema says what
/// a section describes, and the grid is what decides what appears. A plain
/// `ui.label` carries its text as the node's **value** with role `Label`, and a
/// checkbox carries it as the node's label with role `CheckBox` — egui's own
/// mapping in `Response::write_accesskit_node`. This reads the `Label` role
/// only, which is what "one label per text box" is about: a checkbox named
/// "Script" is a different widget from a text box labelled "Script", and
/// counting both would report a duplicate where a modder sees two unrelated
/// controls.
///
/// A galley also emits a `TextRun` node holding the same words — that is how
/// egui supports text selection, and it exists for every piece of rendered
/// text, headings included. It is excluded for the same reason.
fn label_text(
    session: &mut Session,
    state: &mut ObjectEditorState,
    size: nvre_editor::egui::Vec2,
) -> Vec<String> {
    use nvre_editor::egui;
    let mut pages = Pages::default();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut out_text = Vec::new();
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
        out_text = out
            .platform_output
            .accesskit_update
            .map(|u| {
                u.nodes
                    .into_iter()
                    .filter(|(_, node)| {
                        node.role() == nvre_editor::egui::accesskit::Role::Label
                    })
                    .filter_map(|(_, node)| node.value().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
    }
    out_text
}

/// The labels and screen rects of every checkbox the creature editor draws.
///
/// The rect is what says whether the checkboxes are laid out as checkboxes a
/// modder can read or crammed into one cell until their labels are cut off.
fn checkbox_rects(
    session: &mut Session,
    state: &mut ObjectEditorState,
    size: nvre_editor::egui::Vec2,
) -> Vec<(String, nvre_editor::egui::Rect)> {
    use nvre_editor::egui;
    let mut pages = Pages::default();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut out_boxes = Vec::new();
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
        out_boxes = out
            .platform_output
            .accesskit_update
            .map(|u| {
                u.nodes
                    .into_iter()
                    .filter_map(|(_, node)| {
                        let label = node.label()?;
                        let rect = node.bounds()?;
                        let r = egui::Rect::from_min_max(
                            egui::pos2(rect.x0 as f32, rect.y0 as f32),
                            egui::pos2(rect.x1 as f32, rect.y1 as f32),
                        );
                        Some((label.to_string(), r))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
    }
    out_boxes
}

/// The labels of every checkbox the creature editor draws.
fn checkbox_labels(session: &mut Session, state: &mut ObjectEditorState) -> Vec<String> {
    checkbox_rects(session, state, nvre_editor::egui::vec2(900.0, 2400.0))
        .into_iter()
        .map(|(label, _)| label)
        .collect()
}

#[test]
fn the_flag_checkboxes_wrap_onto_their_own_rows() {
    // Ten long flag names do not fit one row in a window a third of the old
    // size. Drawn inside the two-column field grid they were a single cell
    // holding ten widgets side by side, and every label was truncated to fit.
    // The checkboxes now take the row and wrap down it.
    //
    // The assertion is geometric: at a width that cannot hold all ten on one
    // line, the flag checkboxes must occupy more than one distinct y. A single
    // over-wide row — the old layout — has one y for all of them.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("General".to_string());
    let bits = |key: &str| -> Vec<u32> {
        session
            .schema
            .page_sections("CREA")
            .iter()
            .filter(|s| s.tab.as_deref() == Some("General"))
            .filter_map(|s| s.flags_bits.get(key).cloned())
            .next()
            .unwrap_or_default()
    };
    let flag_bits = bits("ACBS.flags");
    assert_eq!(flag_bits.len(), 10, "General names ten flags");

    let acbs = session
        .schema
        .records
        .get("CREA")
        .and_then(|r| r.subrecords.get("ACBS"))
        .expect("ACBS");
    let nvre_esm::TypeDef::Struct(fields) = session.schema.resolve_def(&acbs.ty) else {
        panic!("ACBS is a struct");
    };
    let nvre_esm::TypeDef::Flags { bits: named, .. } = &fields
        .iter()
        .find(|f| f.name == "flags")
        .expect("ACBS.flags")
        .ty
    else {
        panic!("ACBS.flags is a flags word");
    };
    let flag_names: Vec<String> = flag_bits
        .iter()
        .filter_map(|b| named.iter().find(|(bit, _)| bit == b).map(|(_, n)| n.clone()))
        .collect();

    // Narrow enough that ten long names cannot share a line.
    let boxes = checkbox_rects(&mut session, &mut state, nvre_editor::egui::vec2(760.0, 2400.0));
    let flags: Vec<&nvre_editor::egui::Rect> = boxes
        .iter()
        .filter(|(label, _)| flag_names.iter().any(|n| n == label))
        .map(|(_, r)| r)
        .collect();
    assert_eq!(
        flags.len(),
        flag_names.len(),
        "every flag on General is a checkbox: found {} of {}",
        flags.len(),
        flag_names.len()
    );
    // Distinct rows, counting a y as the same line when it is within a line
    // height: the ten names have different heights only by their own glyphs.
    let mut rows: Vec<f32> = flags.iter().map(|r| r.min.y).collect();
    rows.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let distinct = rows
        .iter()
        .fold((0usize, f32::NEG_INFINITY), |(n, last), y| {
            if y - last > 8.0 {
                (n + 1, *y)
            } else {
                (n, last)
            }
        })
        .0;
    assert!(
        distinct > 1,
        "the ten flag checkboxes all sit on one row ({flags:?}) at 760pt wide — they \\
         are crammed into one cell and their labels are truncated. They must wrap"
    );
}

#[test]
fn the_identity_section_draws_no_title_and_no_bare_signature_labels() {
    // The Identity section used to print a bold "Identity" heading and, inside
    // it, three rows labelled with the literal word "value" — a scalar
    // subrecord's row label was its empty field path rendered as text. What a
    // modder needs is the schema's own name for each subrecord.
    //
    // The assertion reads what the page actually drew, through egui's
    // accessibility tree, rather than what the schema describes.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("General".to_string());
    let drawn = label_text(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    let drawn: Vec<&str> = drawn.iter().map(String::as_str).collect();

    // The heading is gone: nothing on the tab says "Identity".
    assert!(
        !drawn.contains(&"Identity"),
        "the Identity section still draws its heading: {drawn:?}"
    );
    // The rows name themselves, under the schema's own names.
    for want in ["Editor ID", "Name"] {
        assert!(
            drawn.contains(&want),
            "the identity section draws no {want:?} row: {drawn:?}"
        );
    }
    // **One** label per text box. Each used to carry three: the subrecord's
    // heading ("Editor ID EDID"), the row label ("Editor ID"), and the box. The
    // subrecord heading is gone, so each name appears exactly once — and the
    // redundant signature beside it is gone with it.
    for want in ["Editor ID", "Name"] {
        let count = drawn.iter().filter(|l| **l == want).count();
        assert_eq!(
            count, 1,
            "{want:?} is drawn {count} times — one label per text box, not three: {drawn:?}"
        );
    }
    // The script is not here: it moved to the Scripts tab, where GECK puts it.
    assert!(
        !drawn.contains(&"Script"),
        "the General tab still draws the script: {drawn:?}"
    );
    // And the bare signature / placeholder labels are gone. `value` is what an
    // empty field path used to print; the four-byte tags are what a subrecord
    // with no name would fall back to.
    for junk in ["value", "EDID", "FULL", "SCRI"] {
        assert!(
            !drawn.contains(&junk),
            "the identity section still draws the raw label {junk:?}: {drawn:?}"
        );
    }
    // The other sections on General still carry their headings — the change is
    // per section, not a page-wide removal.
    for keep in ["Actor Flags", "Template Data"] {
        assert!(
            drawn.contains(&keep),
            "General no longer draws its {keep:?} heading: {drawn:?}"
        );
    }
}

#[test]
fn the_template_picker_comes_before_the_template_checkboxes() {
    // GECK puts the ActorBase picker at the top of the Template group, above the
    // checkboxes that say what the chosen template contributes. The section used
    // to list `ACBS` first, which drew the ten checkboxes and the picker below
    // them — the picker is the entry point to the section, not its footnote.
    //
    // The assertion reads where each widget landed, because the section's `subs`
    // order is only a claim about what the renderer does with it.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("General".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    // Everything below the "Template Data" heading and above the next group, so
    // the assertion is about this section and not about the Identity section's
    // own picker — which reports "NONE" too, because it is empty as well.
    let heading = rows
        .iter()
        .find(|(text, _)| text == "Template Data")
        .map(|(_, y)| *y)
        .unwrap_or_else(|| panic!("the Template Data heading is not drawn: {rows:?}"));
    let section: Vec<&(String, f32)> = rows
        .iter()
        .filter(|(_, y)| *y > heading)
        .take_while(|(text, _)| text != "Everything else")
        .collect();
    let at = |want: &str| -> f32 {
        section
            .iter()
            .find(|(text, _)| text == want)
            .map(|(_, y)| *y)
            .unwrap_or_else(|| {
                panic!("{want:?} is not drawn in the Template Data section: {section:?}")
            })
    };
    let picker = at("NONE");
    let template_row_y = at("Traits");
    assert!(
        picker < template_row_y,
        "the template picker is at y={picker}, below the first template checkbox at \
         y={template_row_y}. It must come first: {section:?}"
    );
}

#[test]
fn a_flags_field_draws_no_row_label_of_its_own() {
    // The section heading already says "Actor Flags" and the checkboxes name
    // themselves, so the field's path was a third name for the same thing. What
    // a modder saw was a lone line reading "flags" above ten checkboxes, and
    // "template_flags" above ten more.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("General".to_string());
    let drawn = label_text(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    for path in ["flags", "template_flags"] {
        assert!(
            !drawn.iter().any(|l| l == path),
            "the {path:?} field still draws its own row label: {drawn:?}"
        );
    }
    // The headings stay — it is the redundant per-field label that goes.
    for keep in ["Actor Flags", "Template Data"] {
        assert!(
            drawn.iter().any(|l| l == keep),
            "General no longer draws its {keep:?} heading: {drawn:?}"
        );
    }
}

/// Every drawn widget's text and its vertical position, top to bottom.
///
/// The position is what proves an ordering: a section's `subs` list says which
/// subrecords it names and in what order, and this says whether the renderer
/// put them where that order claims. Labels, checkboxes and combo boxes are
/// read, which covers the identity rows, the flag checkboxes and the pickers.
fn placed_widgets(
    session: &mut Session,
    state: &mut ObjectEditorState,
    size: nvre_editor::egui::Vec2,
) -> Vec<(String, f32)> {
    use nvre_editor::egui;
    let mut pages = Pages::default();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut rows: Vec<(String, f32)> = Vec::new();
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
        rows = out
            .platform_output
            .accesskit_update
            .map(|u| {
                let mut v: Vec<(String, f32)> = u
                    .nodes
                    .into_iter()
                    .filter(|(_, n)| {
                        matches!(
                            n.role(),
                            egui::accesskit::Role::Label
                                | egui::accesskit::Role::CheckBox
                                | egui::accesskit::Role::ComboBox
                        )
                    })
                    .filter_map(|(_, n)| {
                        let text = n.label().or_else(|| n.value())?;
                        let rect = n.bounds()?;
                        Some((text.to_string(), rect.y0 as f32))
                    })
                    .collect();
                v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                v
            })
            .unwrap_or_default();
    }
    rows
}

#[test]
fn a_creature_with_no_template_still_gets_the_picker() {
    // 428 of Fallout: New Vegas's 1,578 creatures carry no `TPLT` — mostly the
    // `AudioTemplate*` records. The sectioned grid skips a subrecord the record
    // does not carry, which is right for `VATS` on a Fallout 4 weapon and wrong
    // here: GECK's creature dialog is a fixed form, and the Template dropdown is
    // there on a creature with no template precisely so a modder can give it
    // one. A subrecord nobody draws is a subrecord nobody can set, so those
    // creatures had no way to gain a template at all.
    //
    // The picker is drawn, the record's other fields are still drawn, and
    // picking one writes the subrecord the record did not have.
    let Some((mut session, _)) = open_creature() else {
        return;
    };
    // A creature with no TPLT, so the test is about the absent case rather than
    // about the fixture, which happens to carry one.
    let fid = session
        .records_of(*b"CREA")
        .iter()
        .find(|(f, _)| {
            session
                .resolved_record(*f)
                .and_then(|r| r.subrecords().ok())
                .is_some_and(|s| !s.iter().any(|x| &x.sig == b"TPLT"))
        })
        .map(|(f, _)| *f)
        .unwrap_or_else(|| panic!("every creature in the master carries a TPLT"));
    session.open_object(fid, *b"CREA");
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid, *b"CREA");
    state.tab.chosen = Some("General".to_string());

    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    let heading = rows
        .iter()
        .find(|(text, _)| text == "Template Data")
        .map(|(_, y)| *y)
        .unwrap_or_else(|| panic!("the Template Data section is not drawn: {rows:?}"));
    let section: Vec<&(String, f32)> = rows
        .iter()
        .filter(|(_, y)| *y > heading)
        .take_while(|(text, _)| text != "Everything else")
        .collect();

    // The picker is there, and it says why the row is unusual.
    assert!(
        section.iter().any(|(t, _)| t == "NONE"),
        "no template picker on a creature with no TPLT: {section:?}"
    );
    assert!(
        section.iter().any(|(t, _)| t == "is not on this record"),
        "the absent subrecord is not marked as such: {section:?}"
    );
    // And the ten template checkboxes are still drawn — skipping the section
    // would have taken them with it.
    for want in ["Traits", "Stats", "Script", "Inventory"] {
        assert!(
            section.iter().any(|(t, _)| t == want),
            "the template checkbox {want:?} is gone: {section:?}"
        );
    }

    // Picking one writes the subrecord the record did not have.
    let donor = session
        .records_of(*b"CREA")
        .iter()
        .find(|(f, _)| {
            session
                .resolved_record(*f)
                .and_then(|r| r.subrecords().ok())
                .is_some_and(|s| s.iter().any(|x| &x.sig == b"TPLT"))
        })
        .map(|(f, _)| *f)
        .expect("a creature that carries a TPLT");
    session.doc.set_sub(fid, b"TPLT", donor.to_le_bytes().to_vec());
    let written = session
        .resolved_record(fid)
        .expect("the record resolves")
        .subrecords()
        .expect("subrecords parse")
        .iter()
        .find(|s| &s.sig == b"TPLT")
        .map(|s| u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]));
    assert_eq!(
        written,
        Some(donor),
        "the picker's choice did not land on the record"
    );
}

#[test]
fn the_object_bounds_and_the_model_path_live_on_the_model_list_tab() {
    // The header used to carry the model path and the object bounds. Both are
    // properties of the model rather than of the record, and a header is drawn
    // on every tab of every record type that carries one — so both were on
    // every page of the dialog. They are on the Model List tab now, beside the
    // model's other subrecords.
    //
    // The header no longer writes them either, which is the part that matters
    // more than the layout: the Model List tab edits them straight into the
    // document, and `set_sub` *replaces* an existing edit for a subrecord. A
    // header that still queued its own copy would overwrite whatever the tab
    // had just changed, with the value it was loaded with — silently, on Apply.
    //
    // So the test is not only that they moved. It is that an edit made on the
    // Model List tab survives an Apply that follows it.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    assert!(
        state.raw_subs.contains_key(b"OBND"),
        "the fixture carries no OBND, so this proves nothing"
    );

    // On the Model List tab, and not on any other.
    state.tab.chosen = Some("ModelList".to_string());
    let on_model_list =
        placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    for want in [
        "Model FileName",
        "min.x",
        "min.y",
        "min.z",
        "max.x",
        "max.y",
        "max.z",
    ] {
        assert!(
            on_model_list.iter().any(|(t, _)| t == want),
            "{want:?} is not on the Model List tab: {on_model_list:?}"
        );
    }

    // The header is drawn above every page, so every other tab's rows include
    // it. None of them may show the path or the bounds.
    for tab in ["General", "Scripts", "Traits"] {
        state.tab.chosen = Some(tab.to_string());
        let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
        for gone in ["Model Path (.nif):", "Object Bounds (OBND)", "Min:", "Max:"] {
            assert!(
                !rows.iter().any(|(t, _)| t == gone),
                "the {tab} tab still draws {gone:?} in the header: {rows:?}"
            );
        }
    }

    // And the header's own copy of the six numbers does not come back on Apply.
    // `state.obnd_min` is loaded from the record and nothing draws it now, so
    // its value is whatever the record already had — writing it would be a
    // no-op on the bytes but would *replace* the Model List tab's edit in the
    // document, which is the bug being ruled out.
    state.tab.chosen = Some("ModelList".to_string());
    let before = payload(&subs(&session, BRAHMIN), b"OBND");
    assert_eq!(before.len(), 12, "OBND is twelve bytes: two Vec3i16");
    let mut edited = before.clone();
    edited[0..2].copy_from_slice(&(-1234i16).to_le_bytes());
    edited[10..12].copy_from_slice(&(4321i16).to_le_bytes());
    session.doc.set_sub(BRAHMIN, b"OBND", edited.clone());
    // A model path edit, queued the same way the tab queues it.
    let original_model = payload(&subs(&session, BRAHMIN), b"MODL");
    session
        .doc
        .set_sub(BRAHMIN, b"MODL", b"foo\\bar.nif\0".to_vec());
    // Now apply, which is what the Apply button does.
    state.save_to_session(&mut session);

    let after = payload(&subs(&session, BRAHMIN), b"OBND");
    assert_eq!(
        i16::from_le_bytes([after[0], after[1]]),
        -1234,
        "the bounds edit on the Model List tab was overwritten by the header: {after:02X?}"
    );
    assert_eq!(
        i16::from_le_bytes([after[10], after[11]]),
        4321,
        "max.z was overwritten by the header: {after:02X?}"
    );
    let model_after = payload(&subs(&session, BRAHMIN), b"MODL");
    assert_eq!(
        model_after,
        b"foo\\bar.nif\0".to_vec(),
        "the model path edit on the Model List tab was overwritten by the header"
    );
    // And the subrecord is still writable at all — excluding it from the header
    // must not have made it read-only.
    assert_ne!(model_after, original_model, "the MODL edit did not take");
}
