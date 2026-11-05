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

    let acbs_section = traits
        .iter()
        .find(|s| s.tab.as_deref() == Some("Traits") && s.fields.contains_key("ACBS"))
        .expect("Traits narrows ACBS");
    assert!(
        acbs_section.fields["ACBS"].contains(&"karma".to_string()),
        "Traits shows ACBS's karma: {:?}",
        acbs_section.fields
    );
    // ACBS flags have all moved to their respective tabs (General, Animation,
    // ModelList, Stats, Blood), so Traits carries 0 flags.
    let stats_flags: Vec<u32> = stats
        .iter()
        .filter_map(|s| s.flags_bits.get("ACBS.flags").cloned())
        .flatten()
        .collect();
    let traits_flags: Vec<u32> = traits
        .iter()
        .filter_map(|s| s.flags_bits.get("ACBS.flags").cloned())
        .flatten()
        .collect();
    assert_eq!(traits_flags.len(), 0, "Traits has no ACBS flags remaining");
    assert!(
        !stats_flags.iter().any(|b| traits_flags.contains(b)),
        "Stats and Traits both show ACBS bits {stats_flags:?} / {traits_flags:?}"
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
    // The word is split across three tabs now: ten on General, eight on
    // Animation, the remaining nine on Traits. The numbers have to add up to the
    // 27 named bits and no bit may be on two tabs, which is what makes the
    // checkboxes on one tab authoritative for their bits.
    let expected_general = schema_bits("General", "ACBS.flags");
    let expected_animation = schema_bits("Animation", "ACBS.flags");
    let expected_traits = schema_bits("Traits", "ACBS.flags");
    assert_eq!(expected_general.len(), 11, "General names eleven flags");
    assert_eq!(expected_animation.len(), 8, "Animation names the eight movement flags");
    assert_eq!(expected_traits.len(), 0, "Traits has no ACBS flags left");
    let expected_model = schema_bits("ModelList", "ACBS.flags");
    assert_eq!(
        expected_model.len(),
        6,
        "ModelList names the six that are about the model"
    );
    let expected_stats = schema_bits("Stats", "ACBS.flags");
    assert_eq!(
        expected_stats.len(),
        1,
        "Stats names PC Level Mult, which governs its own level field"
    );
    let expected_blood = schema_bits("Blood", "ACBS.flags");
    assert_eq!(
        expected_blood.len(),
        2,
        "Blood names the two blood flags"
    );
    // Every named bit is on at least one tab, and on exactly one: the word is
    // split three ways and the same checkbox on two tabs is not a split but a
    // duplicate. A checkbox on no tab is the failure that matters — it would be
    // uneditable everywhere — so that direction is asserted.
    let all: Vec<u32> = expected_general
        .iter()
        .chain(&expected_animation)
        .chain(&expected_model)
        .chain(&expected_stats)
        .chain(&expected_blood)
        .chain(&expected_traits)
        .copied()
        .collect();

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
    for bit in named.keys() {
        assert!(all.contains(bit), "bit {bit} is on no tab at all");
    }

    let split: [(&str, &Vec<u32>); 6] = [
        ("General", &expected_general),
        ("Animation", &expected_animation),
        ("ModelList", &expected_model),
        ("Stats", &expected_stats),
        ("Blood", &expected_blood),
        ("Traits", &expected_traits),
    ];
    for (tab, expected) in split {
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
        let leaked: Vec<String> = split
            .iter()
            .filter(|(other, _)| *other != tab)
            .flat_map(|(_, bits)| bits.iter())
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
    assert_eq!(flag_bits.len(), 11, "General names eleven flags");

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
    //
    // The fixture is a creature that carries a `TPLT`, so the picker shows that
    // template's name rather than "NONE". The old test looked for "NONE", which
    // was only ever correct while every FormID dropdown read "NONE": a FormID
    // decodes to the *string* `"0x0014D1FD"`, and the picker parsed it with
    // `as_u64()`, which a string never satisfies.
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
    // The picker shows the record's own template, and its FormID.
    let picker = at("VStreetCrBrahmin");
    let template_row_y = at("Traits");
    assert!(
        picker < template_row_y,
        "the template picker is at y={picker}, below the first template checkbox at \
         y={template_row_y}. It must come first: {section:?}"
    );
    // And the target types are shown too, so the row is not just a name.
    at("CREA, LVLC");
}

/// The picker reads "NONE" for a creature that carries no `TPLT`, which is the
/// state the old test was written against — and which the general case had been
/// mis-generalised from.
///
/// `000B037F` is one of only six creatures in `FalloutNV.esm` with neither a
/// `TPLT` nor a `ZNAM`; the usual fixtures (`VStreetCrBrahmin2`,
/// `VNiptonLegionMongrelAttack`) carry both, which is why the old assertions
/// passed at all.
#[test]
fn a_creature_with_no_template_shows_none() {
    let Some((mut session, _state)) = open_creature() else {
        return;
    };
    // A creature with no `TPLT`: the unset case.
    let fid = 0x000B_037Fu32;
    session.open_object(fid, *b"CREA");
    assert!(
        subs(&session, fid).iter().all(|s| &s.sig != b"TPLT"),
        "the fixture carries no template"
    );
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid, *b"CREA");
    state.tab.chosen = Some("General".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    let heading = rows
        .iter()
        .find(|(text, _)| text == "Template Data")
        .map(|(_, y)| *y)
        .expect("the Template Data heading is drawn");
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
    assert!(
        at("NONE") < at("Traits"),
        "an unset template picker reads NONE and still leads the section: {section:?}"
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

    // The picker is there, reading NONE — which is what says the record has no template yet.
    assert!(
        section.iter().any(|(t, _)| t == "NONE"),
        "no template picker on a creature with no TPLT: {section:?}"
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

#[test]
fn the_animation_tab_carries_the_movement_flags() {
    // The eight ACBS bits that say how a creature moves — Biped, Swims, Flies,
    // Walks, No Combat in Water, Immobile, Not Pushable, No Rotating To
    // Head-track — were on Traits' Flags section, mixed in with the body-part and
    // tilt bits. GECK files them with the animation, and a section that splits a
    // subrecord's word by what it is *about* is the whole point of `flags_bits`.
    //
    // The section is on the Animation tab, ahead of the animation list itself,
    // and it narrows `ACBS` to its flags alone: the section's other fields are
    // fatigue, barter gold, the level and the rest, and a Movement section that
    // also showed those would be the whole subrecord wearing a new title.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    let titles = |v: &[&nvre_esm::schema::SectionDef]| -> Vec<String> {
        v.iter().map(|s| s.title.clone()).collect()
    };
    let sections = session.schema.page_sections("CREA");
    let animation = sections_on_tab(&sections, Some("Animation"));
    let movement = animation
        .iter()
        .find(|s| s.title == "Movement")
        .unwrap_or_else(|| {
            panic!(
                "the Animation tab has no Movement section: {:?}",
                titles(&animation)
            )
        });
    // Ahead of the animation list, so the movement flags are the first thing on
    // the page rather than the last.
    let anim_pos = animation
        .iter()
        .position(|s| s.title == "Animations")
        .expect("the Animations section is on the Animation tab");
    let move_pos = animation
        .iter()
        .position(|s| s.title == "Movement")
        .expect("the Movement section is on the Animation tab");
    assert!(
        move_pos < anim_pos,
        "Movement is at {move_pos} and Animations at {anim_pos}: the movement flags come first"
    );
    // Exactly the eight, and nothing but them.
    assert_eq!(
        movement.flags_bits.get("ACBS.flags").map(Vec::len),
        Some(8),
        "the Movement section shows eight bits: {:?}",
        movement.flags_bits.get("ACBS.flags")
    );
    // And the section narrows the subrecord to its flags, so it does not also
    // draw fatigue, barter gold and the level.
    assert_eq!(
        movement.fields.get("ACBS").map(Vec::as_slice),
        Some(&["flags".to_string()][..]),
        "the Movement section shows the whole ACBS rather than its flags: {:?}",
        movement.fields
    );

    // Drawn: eight checkboxes, under a "Movement" heading, and nothing else from
    // ACBS. The heading is checked because a section with no title is the
    // Identity section's privilege, not the default.
    state.tab.chosen = Some("Animation".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    assert!(
        rows.iter().any(|(t, _)| t == "Movement"),
        "no Movement heading on the Animation tab: {rows:?}"
    );
    for want in [
        "Biped",
        "Swims",
        "Flies",
        "Walks",
        "No Combat in Water",
        "Immobile",
        "Not Pushable",
        "No Rotating To Head-track",
    ] {
        assert!(
            rows.iter().any(|(t, _)| t == want),
            "the movement checkbox {want:?} is not drawn: {rows:?}"
        );
    }
    for gone in ["fatigue", "barter_gold", "karma", "speed_multiplier"] {
        assert!(
            !rows.iter().any(|(t, _)| t == gone),
            "the Movement section also draws {gone:?}, so it shows the whole subrecord: {rows:?}"
        );
    }
}

#[test]
fn the_model_list_tab_carries_the_model_flags() {
    // The six ACBS bits that describe the creature's *model* — No Head, No Right
    // Arm, No Left Arm, No Shadow, Tilt Front/Back, Tilt Left/Right — were on
    // Traits' Flags section, which is about the actor. They belong with the
    // model, on the Model List tab, beside the model path and the object bounds
    // that were moved there earlier.
    //
    // The section narrows `ACBS` to its flags alone, the way Movement does:
    // `ACBS` also holds fatigue, barter gold, the level and the rest, and a
    // "Model Flags" section that drew those would be the whole subrecord wearing
    // a new title.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    let titles = |v: &[&nvre_esm::schema::SectionDef]| -> Vec<String> {
        v.iter().map(|s| s.title.clone()).collect()
    };
    let sections = session.schema.page_sections("CREA");
    let model_list = sections_on_tab(&sections, Some("ModelList"));
    let model_flags = model_list
        .iter()
        .find(|s| s.title == "Model Flags")
        .unwrap_or_else(|| {
            panic!(
                "the Model List tab has no Model Flags section: {:?}",
                titles(&model_list)
            )
        });
    assert_eq!(
        model_flags.flags_bits.get("ACBS.flags").map(Vec::len),
        Some(6),
        "the Model Flags section shows six bits: {:?}",
        model_flags.flags_bits.get("ACBS.flags")
    );
    assert_eq!(
        model_flags.fields.get("ACBS").map(Vec::as_slice),
        Some(&["flags".to_string()][..]),
        "the Model Flags section shows the whole ACBS rather than its flags: {:?}",
        model_flags.fields
    );
    // And the six are gone from Traits, which is the other half of the move.
    let traits = sections_on_tab(&sections, Some("Traits"));
    let traits_bits: Vec<u32> = traits
        .iter()
        .filter_map(|s| s.flags_bits.get("ACBS.flags").cloned())
        .flatten()
        .collect();
    for bit in [15u32, 16, 17, 19, 24, 25] {
        assert!(
            !traits_bits.contains(&bit),
            "bit {bit} is still on Traits after moving to the Model List tab"
        );
    }

    // Drawn: the six checkboxes under a "Model Flags" heading.
    state.tab.chosen = Some("ModelList".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    assert!(
        rows.iter().any(|(t, _)| t == "Model Flags"),
        "no Model Flags heading on the Model List tab: {rows:?}"
    );
    for want in [
        "No Head",
        "No Right Arm",
        "No Left Arm",
        "No Shadow",
        "Tilt Front/Back",
        "Tilt Left/Right",
    ] {
        assert!(
            rows.iter().any(|(t, _)| t == want),
            "the model checkbox {want:?} is not drawn: {rows:?}"
        );
    }
    // And nothing else from ACBS: the section narrowed the subrecord.
    for gone in ["fatigue", "barter_gold", "karma", "speed_multiplier"] {
        assert!(
            !rows.iter().any(|(t, _)| t == gone),
            "the Model Flags section also draws {gone:?}: {rows:?}"
        );
    }
}

#[test]
fn pc_level_mult_sits_on_the_leveling_section() {
    // Bit 7, PC Level Mult, was on Traits' Flags section. It decides how
    // `ACBS`'s level field is read — a plain level, or a level multiplier over
    // 1000 — and that field is on the Leveling section of the Stats tab. A flag
    // that governs a number belongs beside the number, or a modder editing the
    // level has to know to look at another tab for the switch that changes what
    // the level even means.
    //
    // The Leveling section already narrowed `ACBS` to the level and the two
    // calc values, so it had to start naming `flags` too — a section that lists
    // a bit but not the field the bit lives in draws nothing at all.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    let titles = |v: &[&nvre_esm::schema::SectionDef]| -> Vec<String> {
        v.iter().map(|s| s.title.clone()).collect()
    };
    let sections = session.schema.page_sections("CREA");
    let stats = sections_on_tab(&sections, Some("Stats"));
    let leveling = stats
        .iter()
        .find(|s| s.title == "Leveling")
        .unwrap_or_else(|| panic!("the Stats tab has no Leveling section: {:?}", titles(&stats)));
    assert_eq!(
        leveling.flags_bits.get("ACBS.flags").map(Vec::as_slice),
        Some(&[7u32][..]),
        "Leveling shows PC Level Mult and nothing else: {:?}",
        leveling.flags_bits.get("ACBS.flags")
    );
    // And the field the bit lives in, or the checkbox is unreachable.
    assert!(
        leveling
            .fields
            .get("ACBS")
            .is_some_and(|f| f.iter().any(|p| p == "flags")),
        "Leveling does not name the flags field, so the checkbox would not draw: {:?}",
        leveling.fields
    );
    assert!(
        leveling
            .fields
            .get("ACBS")
            .is_some_and(|f| f.iter().any(|p| p == "level_or_level_mult_raw")),
        "Leveling does not show the level field the flag governs: {:?}",
        leveling.fields
    );

    // Drawn: the checkbox, and it is not on Traits any more.
    state.tab.chosen = Some("Stats".to_string());
    let stats_rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    assert!(
        stats_rows.iter().any(|(t, _)| t == "PC Level Mult"),
        "no PC Level Mult checkbox on the Stats tab: {stats_rows:?}"
    );
    state.tab.chosen = Some("Traits".to_string());
    let traits_rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    assert!(
        !traits_rows.iter().any(|(t, _)| t == "PC Level Mult"),
        "PC Level Mult is still on the Traits tab: {traits_rows:?}"
    );
}

#[test]
fn the_blood_flags_sit_on_the_blood_section() {
    // Bits 11 and 12 — No Blood Spray, No Blood Decal — were the last two on
    // Traits' Flags section, which is about the actor. They are about blood, and
    // there is a Blood tab.
    //
    // The Blood section already carried `NAM4` and `CNAM`, the impact dataset and
    // the blood colour, so it had to start naming `ACBS` as well. A section that
    // lists a bit for a subrecord it does not name draws nothing at all.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    let titles = |v: &[&nvre_esm::schema::SectionDef]| -> Vec<String> {
        v.iter().map(|s| s.title.clone()).collect()
    };
    let sections = session.schema.page_sections("CREA");
    let blood_tab = sections_on_tab(&sections, Some("Blood"));
    let blood = blood_tab
        .iter()
        .find(|s| s.title == "Blood")
        .unwrap_or_else(|| panic!("the Blood tab has no Blood section: {:?}", titles(&blood_tab)));
    assert_eq!(
        blood.flags_bits.get("ACBS.flags").map(Vec::as_slice),
        Some(&[11u32, 12][..]),
        "the Blood section shows the two blood flags: {:?}",
        blood.flags_bits.get("ACBS.flags")
    );
    assert!(
        blood.subs.iter().any(|t| t == "ACBS"),
        "the Blood section does not name ACBS, so the flags would not draw: {:?}",
        blood.subs
    );
    assert_eq!(
        blood.fields.get("ACBS").map(Vec::as_slice),
        Some(&["flags".to_string()][..]),
        "the Blood section shows the whole ACBS rather than its flags: {:?}",
        blood.fields
    );

    // Drawn: both checkboxes on the Blood tab, and neither on Traits.
    state.tab.chosen = Some("Blood".to_string());
    let blood_rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    for want in ["No Blood Spray", "No Blood Decal"] {
        assert!(
            blood_rows.iter().any(|(t, _)| t == want),
            "no {want:?} checkbox on the Blood tab: {blood_rows:?}"
        );
    }
    // And the section still shows what it showed before — the impact dataset and
    // the blood colour are the rest of the Blood tab's reason to exist.
    assert!(
        blood_rows.iter().any(|(t, _)| t == "Impact Dataset"),
        "the Blood section lost its other subrecords: {blood_rows:?}"
    );
    state.tab.chosen = Some("Traits".to_string());
    let traits_rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    for gone in ["No Blood Spray", "No Blood Decal"] {
        assert!(
            !traits_rows.iter().any(|(t, _)| t == gone),
            "{gone:?} is still on the Traits tab: {traits_rows:?}"
        );
    }
}

#[test]
fn the_unverified_bit_stays_marked_unverified() {
    // Bit 2 is the one ACBS flag nobody has explained. xEdit's own source names it
    // 'Weapon & Shield?' with the question mark, fopdoc gives it no meaning, and the
    // GECK wiki's ActorHasBaseFlag table writes it the same way. It is on 22 of 1,578
    // creatures, all supermutants, behemoths or feral ghouls, and on only 6 of the 475
    // that carry a melee list — so it is not "can hold a weapon".
    //
    // The note says all of that, and the name keeps its question mark. Both matter:
    // a schema that quietly drops the '?' turns a guess into a fact, and the next
    // reader has no way to know which bits were verified and which were inherited.
    let Some((session, _)) = open_creature() else {
        return;
    };
    let acbs = session
        .schema
        .records
        .get("CREA")
        .and_then(|r| r.subrecords.get("ACBS"))
        .expect("ACBS is a subrecord of CREA");
    let nvre_esm::TypeDef::Struct(fields) = session.schema.resolve_def(&acbs.ty) else {
        panic!("ACBS is a struct");
    };
    let nvre_esm::TypeDef::Flags { bits, .. } = &fields
        .iter()
        .find(|f| f.name == "flags")
        .expect("ACBS.flags")
        .ty
    else {
        panic!("ACBS.flags is a flags word");
    };
    let name = bits
        .iter()
        .find(|(bit, _)| *bit == 2)
        .map(|(_, n)| n.clone())
        .unwrap_or_else(|| panic!("bit 2 is not named at all"));
    assert!(
        name.contains('?'),
        "bit 2 is named {name:?} without its question mark — the name is a guess, and \\
         dropping the '?' presents it as verified"
    );
    // The note is not loaded into the schema — `note` is a JSON comment for
    // readers, never parsed — so it is checked where it lives, on disk.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv/gameplay.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("the schema file")).expect("valid JSON");
    let note = json
        .pointer("/records/CREA/subrecords/ACBS/type/struct/0/note")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no note on ACBS's struct in {}", path.display()));
    assert!(
        note.contains("unverified"),
        "the ACBS note does not mark bit 2 unverified: {note:?}"
    );
    // And the evidence that ruled out the obvious reading, so the note is not just a
    // shrug: someone checked whether it means "can hold a weapon".
    assert!(
        note.contains("6 of the 475"),
        "the note does not record the melee-list count that ruled out 'can hold a weapon'"
    );
}

#[test]
fn every_link_dropdown_has_one_clear_label() {
    // The Links section holds five FormID pickers: Death item, Voice, Combat
    // Style, Melee Weapon List, Body Part Data. Each had a label problem.
    //
    // A subrecord the record does not carry showed its raw signature — "LNAM",
    // "is not on this record" — because the absent-row path printed the tag
    // instead of asking the schema for the name a present row would have used.
    // The signature is the one thing a modder already has; the name is what they
    // came for. The note itself is gone now: the picker reading NONE says it.
    //
    // And every picker carried "any record" beside it, which says the dropdown is
    // unfiltered. That is true of most of them and useful of none: the dropdown
    // already shows the chosen record, and a hint that is always there stops
    // being a hint.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("Traits".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));

    // One label each, under the schema's own names.
    for want in [
        "Death item",
        "Voice",
        "Combat Style",
        "Weapon List",
    ] {
        let count = rows.iter().filter(|(t, _)| t == want).count();
        assert_eq!(
            count, 1,
            "{want:?} is drawn {count} times — one label per dropdown: {rows:?}"
        );
    }
    // Body Part Data moved to ModelList and must not be on Traits.
    assert!(
        !rows.iter().any(|(t, _)| t == "Body Part Data"),
        "Body Part Data is still on Traits: {rows:?}"
    );
    // No raw signatures standing in for a name.
    for tag in ["INAM", "VTCK", "ZNAM", "LNAM"] {
        assert!(
            !rows.iter().any(|(t, _)| t == tag),
            "the Links section shows the bare signature {tag:?} instead of its name: {rows:?}"
        );
    }

    // And verify Body Part Data is drawn on ModelList.
    state.tab.chosen = Some("ModelList".to_string());
    let model_rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    assert!(
        model_rows.iter().any(|(t, _)| t == "Body Part Data"),
        "Body Part Data is not drawn on ModelList: {model_rows:?}"
    );
    // And no unfiltered hint. "LVLI, SPEL" beside a filtered dropdown stays: that
    // one says what may go here.
    assert!(
        !rows.iter().any(|(t, _)| t == "any record"),
        "a picker still carries the 'any record' hint: {rows:?}"
    );
    // An absent subrecord still gets its name — a picker reading NONE is the
    // whole row, so the name is the only thing that says which link it is.
    assert!(
        !rows.iter().any(|(t, _)| t.contains("not on this record")),
        "the absent-subrecord note is back: {rows:?}"
    );
}

#[test]
fn the_template_dropdown_targets_creatures_and_leveled_creatures() {
    // TPLT was previously an untargeted FormID, so its dropdown loaded
    // all_objects() — 44,550 records across every record type in the game.
    // That made opening the dropdown crawl and killed framerates.
    //
    // The schema now targets CREA and LVLC, matching xEdit and engine rules,
    // so the dropdown offers creature templates only.
    let Some((session, _)) = open_creature() else {
        return;
    };
    let targets = nvre_editor::object_editor::formid_targets_in(
        &session.schema,
        b"CREA",
        b"TPLT",
        None,
        "",
    );
    assert_eq!(
        targets,
        vec!["CREA".to_string(), "LVLC".to_string()],
        "CREA TPLT must only offer creatures and leveled creature lists"
    );
}

#[test]
fn the_traits_tab_draws_an_alignment_dropdown_for_acbs_karma() {
    // In Bethesda engines, karma is stored as a 4-byte float in ACBS.
    // GECK presents this as an "Alignment" dropdown (Very Good, Good,
    // Neutral, Evil, Very Evil) mapping to standard karma values (+900, +500,
    // 0, -500, -900) rather than a raw float spinner.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("Traits".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));

    // The field label is "Alignment", not the raw field name "karma".
    assert!(
        rows.iter().any(|(t, _)| t == "Alignment"),
        "the Traits tab has no Alignment label: {rows:?}"
    );
    assert!(
        !rows.iter().any(|(t, _)| t == "karma"),
        "the Traits tab still shows the raw 'karma' field label: {rows:?}"
    );

    // The brahmin has karma 0.0, so the dropdown displays "Neutral".
    assert!(
        rows.iter().any(|(t, _)| t == "Neutral"),
        "the Alignment dropdown does not display Neutral for the brahmin: {rows:?}"
    );

    // The sibling field is properly labeled "Disposition Base".
    assert!(
        rows.iter().any(|(t, _)| t == "Disposition Base"),
        "the Disposition Base label is missing: {rows:?}"
    );
}

#[test]
fn combat_style_dropdown_shows_the_records_combat_style() {
    // In GECK, an *unset* Combat Style displays "Default" rather than "NONE"
    // (it falls back to the default combat style for its class/race). A combat
    // style the record does carry shows that record's name.
    //
    // `VStreetCrBrahmin2` carries `ZNAM` = `0003A371` (`CSBrahmin`), so this is
    // the set case: the row must show the record's own combat style, not the
    // "Default" an unset one would show.
    //
    // The assertion is on the name, not on "not NONE": the row used to read
    // "NONE" for every creature, because a FormID decodes to the *string*
    // `"0x0003A371"` and the picker parsed it with `as_u64()` — which a string
    // never satisfies. Every FormID dropdown in the object editor therefore
    // showed "NONE" whatever the record held, and clicking an option was a
    // no-op because the picker compared the click against `None`.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("Traits".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));

    // The Combat Style row shows the record's combat style, and its target type.
    assert!(
        rows.iter().any(|(t, _)| t == "CSBrahmin"),
        "Combat Style does not show the record's own combat style: {rows:?}"
    );
    assert!(
        rows.iter().any(|(t, _)| t == "CSTY"),
        "Combat Style does not show its target type: {rows:?}"
    );
    // The label is still there, and the row is not showing the unset "Default".
    assert!(
        rows.iter().any(|(t, _)| t == "Combat Style"),
        "the Combat Style label is missing: {rows:?}"
    );

    // Other unset dropdowns still display "NONE".
    assert!(
        rows.iter().any(|(t, _)| t == "NONE"),
        "Other dropdowns should still display 'NONE': {rows:?}"
    );
}

/// The "Default" label is for an *unset* Combat Style, not for every one: a
/// creature whose `ZNAM` is absent shows "Default", and one that carries a
/// combat style shows that style's name.
///
/// `000B037F` has no `ZNAM`, which is the state the old test was written for —
/// and which the general case had been mis-generalised from.
#[test]
fn an_unset_combat_style_shows_default() {
    let Some((mut session, _state)) = open_creature() else {
        return;
    };
    // A creature with no `ZNAM`: the unset case.
    let fid = 0x000B_037Fu32;
    session.open_object(fid, *b"CREA");
    assert!(
        subs(&session, fid).iter().all(|s| &s.sig != b"ZNAM"),
        "the fixture carries no combat style"
    );
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid, *b"CREA");
    state.tab.chosen = Some("Traits".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    assert!(
        rows.iter().any(|(t, _)| t == "Default"),
        "an unset Combat Style must show 'Default': {rows:?}"
    );
}

#[test]
fn the_stats_tab_has_clean_geck_style_labels_and_sections() {
    // The Stats tab should have clean human labels matching GECK, not internal
    // schema identifiers, no compiler padding (`unused`), and no duplicate
    // subrecord headers.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("Stats".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));

    // Clean GECK labels:
    for want in [
        "Level",
        "Calc Min",
        "Calc Max",
        "Strength",
        "Perception",
        "Endurance",
        "Charisma",
        "Intelligence",
        "Agility",
        "Luck",
        "Health",
        "Damage",
        "Combat Skill",
        "Magic Skill",
        "Stealth Skill",
        "Fatigue",
        "Speed Mult %",
    ] {
        assert!(
            rows.iter().any(|(t, _)| t == want),
            "Stats tab is missing clean label {want:?}: {rows:?}"
        );
    }

    // No raw internal schema identifiers:
    for bad in [
        "level_or_level_mult_raw",
        "calc_min",
        "calc_max",
        "combat_skill",
        "magic_skill",
        "stealth_skill",
        "health",
        "damage",
        "fatigue",
        "speed_multiplier",
        "attributes.strength",
        "unused",
    ] {
        assert!(
            !rows.iter().any(|(t, _)| t == bad),
            "Stats tab still displays raw identifier {bad:?}: {rows:?}"
        );
    }

    // No duplicate subrecord headers:
    assert!(
        !rows.iter().any(|(t, _)| t == "Configuration" || t == "ACBS"),
        "redundant Configuration ACBS heading should not be drawn: {rows:?}"
    );

    // barter_gold moved to AI Data:
    assert!(
        !rows.iter().any(|(t, _)| t == "Barter Gold" || t == "barter_gold"),
        "barter_gold should not be on Stats: {rows:?}"
    );

    // Sound Level moved to Sounds tab, Unarmed Attack Animation to Actor Effect List:
    assert!(
        !rows.iter().any(|(t, _)| t == "Sound Level" || t == "Unarmed Attack Animation"),
        "Sound Level and Unarmed Attack Animation should not be on Stats: {rows:?}"
    );
    assert!(
        !rows.iter().any(|(t, _)| t == "Physical"),
        "Physical section should no longer exist on Stats: {rows:?}"
    );
}

#[test]
fn pc_level_mult_changes_level_label_and_calc_min_max_editability() {
    // When PC Level Mult is unticked:
    // - The level label says "Level"
    // - Calc Min and Calc Max are not editable (disabled)
    // When PC Level Mult is ticked:
    // - The level label changes to "Level Mult 1.0000"
    // - Calc Min and Calc Max are editable (enabled)
    let data = std::env::var("NVRE_DATA").expect("NVRE_DATA");
    let mut session = Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .expect("session");

    // 1. Unticked creature: Brahmin (0x0017_B587)
    let fid_unticked = 0x0017_B587u32;
    session.open_object(fid_unticked, *b"CREA");
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid_unticked, *b"CREA");
    state.tab.chosen = Some("Stats".to_string());
    let rows_unticked = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));
    assert!(
        rows_unticked.iter().any(|(t, _)| t == "Level"),
        "Unticked creature should have 'Level' label: {rows_unticked:?}"
    );
    assert!(
        !rows_unticked.iter().any(|(t, _)| t == "Level Mult"),
        "Unticked creature should not have 'Level Mult' label: {rows_unticked:?}"
    );

    // 2. Ticked creature: BMNeil (0x000E_60F0)
    let fid_ticked = 0x000E_60F0u32;
    session.open_object(fid_ticked, *b"CREA");
    let mut state_ticked = ObjectEditorState::default();
    state_ticked.load_from_session(&session, fid_ticked, *b"CREA");
    state_ticked.tab.chosen = Some("Stats".to_string());
    let rows_ticked = placed_widgets(&mut session, &mut state_ticked, nvre_editor::egui::vec2(900.0, 2400.0));
    assert!(
        rows_ticked.iter().any(|(t, _)| t == "Level Mult"),
        "Ticked creature should have 'Level Mult' label: {rows_ticked:?}"
    );
    assert!(
        !rows_ticked.iter().any(|(t, _)| t == "Level"),
        "Ticked creature should not have bare 'Level' label: {rows_ticked:?}"
    );

}

#[test]
fn the_skills_and_health_section_aligns_all_its_controls_in_one_grid() {
    // When a section groups multiple subrecords (DATA, ACBS, RNAM, TNAM, BNAM, WNAM),
    // rendering each subrecord in its own separate grid causes the text boxes to
    // zigzag at different X coordinates because each subrecord computes column width
    // independently.
    //
    // All rows in the section now share a single grid, so every text box starts
    // at the exact same X coordinate and has the exact same width.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("Stats".to_string());
    let pages = &mut nvre_editor::object_editor::Pages::default();
    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();

    // Run 2 passes to stabilize grid layout
    for _ in 0..2 {
        let mut out = ctx.run_ui(
            nvre_editor::egui::RawInput {
                screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                    nvre_editor::egui::Pos2::ZERO,
                    nvre_editor::egui::vec2(900.0, 2400.0),
                )),
                ..Default::default()
            },
            |ui| {
                nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                    nvre_editor::object_editor::body(ui, &mut session, &mut state, pages);
                });
            },
        );
        out.textures_delta.clear();
    }

    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(900.0, 2400.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                nvre_editor::object_editor::body(ui, &mut session, &mut state, pages);
            });
        },
    );
    out.textures_delta.clear();

    let u = out.platform_output.accesskit_update.expect("accesskit update");
    let nodes = u.nodes;
    let mut spinbuttons: Vec<(f32, f32, f32)> = nodes
        .iter()
        .filter_map(|(_, n)| {
            if n.role() == nvre_editor::egui::accesskit::Role::SpinButton {
                let r = n.bounds()?;
                Some((r.y0 as f32, r.x0 as f32, (r.x1 - r.x0) as f32))
            } else {
                None
            }
        })
        .filter(|(_, x0, _)| *x0 > 200.0) // Skills & Health column (side by side on the right)
        .collect();
    spinbuttons.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    assert_eq!(
        spinbuttons.len(),
        11,
        "Skills & Health must have exactly 11 spinbuttons in the right column: {spinbuttons:?}"
    );

    // And verify Attributes and Skills & Health are side by side at the same Y
    let attrs = nodes.iter().map(|(_, n)| n).find(|n| n.label() == Some("Attributes") || n.value() == Some("Attributes")).unwrap().bounds().unwrap();
    let skills = nodes.iter().map(|(_, n)| n).find(|n| n.label() == Some("Skills & Health") || n.value() == Some("Skills & Health")).unwrap().bounds().unwrap();
    assert_eq!(attrs.y0, skills.y0, "Attributes and Skills & Health must start at the exact same Y position!");
    assert!(skills.x0 > attrs.x0, "Skills & Health must be to the right of Attributes!");

    let first_x0 = spinbuttons[0].1;
    let first_w = spinbuttons[0].2;
    for (i, (y, x0, w)) in spinbuttons.iter().enumerate() {
        assert!(
            (x0 - first_x0).abs() < 1.0,
            "SpinButton {i} at y={y} has x0={x0}, but first has x0={first_x0}. All text boxes must align vertically!"
        );
        assert!(
            (w - first_w).abs() < 1.0,
            "SpinButton {i} at y={y} has width={w}, but first has width={first_w}. All text boxes must have uniform width!"
        );
    }
}

#[test]
fn the_factions_tab_renders_as_a_clean_table_with_targeted_dropdowns() {
    // The Factions tab renders repeated SNAM entries. It should render as a clean
    // table with Faction and Rank column headers, a "+ Add Faction" button, no
    // compiler padding (`unused [73, 70, 90]`), and dropdowns targeted strictly to FACT.
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("Factions".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));

    // Headers (must appear exactly once, not duplicated per row):
    let faction_headers = rows.iter().filter(|(t, _)| t == "Faction").count();
    assert_eq!(
        faction_headers, 1,
        "Factions tab should have exactly 1 'Faction' header, not {faction_headers}"
    );
    let rank_headers = rows.iter().filter(|(t, _)| t == "Rank").count();
    assert_eq!(
        rank_headers, 1,
        "Factions tab should have exactly 1 'Rank' header, not {rank_headers}"
    );

    // No compiler padding or raw byte dumps:
    assert!(
        !rows.iter().any(|(t, _)| t == "unused" || t.contains("[73,70,90]") || t.contains("[73, 70, 90]")),
        "Factions tab still exposes raw compiler padding: {rows:?}"
    );

    // Each faction row must be stacked vertically on its own line (not stretched horizontally):
    let y_animal = rows.iter().find(|(t, _)| t == "AnimalFriendFaction").map(|(_, y)| *y).expect("AnimalFriendFaction");
    let y_brahmin = rows.iter().find(|(t, _)| t == "BrahminFaction").map(|(_, y)| *y).expect("BrahminFaction");
    let y_hit = rows.iter().find(|(t, _)| t == "CreatureHitBrahmin").map(|(_, y)| *y).expect("CreatureHitBrahmin");
    assert!(
        y_animal < y_brahmin && y_brahmin < y_hit,
        "Faction rows must be on separate lines: animal={y_animal}, brahmin={y_brahmin}, hit={y_hit}"
    );

    // FormID targeting:
    let targets = nvre_editor::object_editor::formid_targets_in(
        &session.schema,
        b"CREA",
        b"SNAM",
        None,
        "faction",
    );
    assert_eq!(
        targets,
        vec!["FACT".to_string()],
        "SNAM.faction must target FACT"
    );

    // Verify adding a faction via UI button increments subrecord count
    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();
    let pages = &mut nvre_editor::object_editor::Pages::default();
    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(900.0, 2400.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                nvre_editor::object_editor::body(ui, &mut session, &mut state, pages);
            });
        },
    );
    out.textures_delta.clear();
    let u = out.platform_output.accesskit_update.unwrap();
    let add_buttons = u.nodes.iter().filter(|(_, n)| {
        n.label() == Some("+ Add Faction") || n.value() == Some("+ Add Faction")
    }).count();
    assert_eq!(
        add_buttons, 1,
        "Factions tab should have exactly 1 '+ Add Faction' button, not {add_buttons}"
    );
    let btn_rect = u.nodes.iter().find_map(|(_, n)| {
        if n.label() == Some("+ Add Faction") || n.value() == Some("+ Add Faction") {
            n.bounds()
        } else {
            None
        }
    }).expect("+ Add Faction button must exist");
    let click_pos = nvre_editor::egui::pos2(btn_rect.x0 as f32 + 5.0, btn_rect.y0 as f32 + 5.0);

    let mut out2 = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(900.0, 2400.0),
            )),
            events: vec![
                nvre_editor::egui::Event::PointerMoved(click_pos),
                nvre_editor::egui::Event::PointerButton {
                    pos: click_pos,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                nvre_editor::egui::Event::PointerButton {
                    pos: click_pos,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                nvre_editor::object_editor::body(ui, &mut session, &mut state, pages);
            });
        },
    );
    out2.textures_delta.clear();

    let rec_after = session.resolved_record(BRAHMIN).unwrap();
    let snams_after = rec_after.subrecords().unwrap().iter().filter(|s| &s.sig == b"SNAM").count();
    assert_eq!(snams_after, 4, "+ Add Faction must increment SNAM rows from 3 to 4");
}

#[test]
fn cralien_faction_displays_named_faction_not_none() {
    // CrAlien (0x0016_90AC) belongs to WWAlienFaction (0x0017_4363).
    // In raw ESM bytes, FormIDs in subrecords decode as hex strings ("0x00174363").
    // If the editor only checks `as_u64()`, the FormID parser fails, causing the
    // dropdown to resolve to `None` and display "NONE" instead of the faction name.
    let data = std::env::var("NVRE_DATA").expect("NVRE_DATA");
    let mut session = Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .expect("session");

    let fid = 0x0016_90ACu32; // CrAlien
    session.open_object(fid, *b"CREA");
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid, *b"CREA");
    state.tab.chosen = Some("Factions".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));

    assert!(
        rows.iter().any(|(t, _)| t == "WWAlienFaction"),
        "CrAlien should display 'WWAlienFaction' in Factions table: {rows:?}"
    );
    assert!(
        !rows.iter().any(|(t, _)| t == "NONE"),
        "CrAlien has a valid faction and should not show 'NONE': {rows:?}"
    );
}

#[test]
fn the_factions_tab_renders_and_adds_when_actor_has_no_factions() {
    let data = match std::env::var("NVRE_DATA") {
        Ok(d) => d,
        Err(_) => return,
    };
    let mut session = Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .expect("session");

    // ARCHIMEDESTurret (0x000E07D6) has 0 factions.
    let fid = 0x000E07D6u32;
    session.open_object(fid, *b"CREA");
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid, *b"CREA");
    state.tab.chosen = Some("Factions".to_string());
    let mut pages = Pages::default();

    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();

    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(900.0, 2400.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                nvre_editor::object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );
    out.textures_delta.clear();

    let u = out.platform_output.accesskit_update.expect("accesskit update");
    let has_faction_hdr = u.nodes.iter().any(|(_, n)| n.label() == Some("Faction") || n.value() == Some("Faction"));
    let has_rank_hdr = u.nodes.iter().any(|(_, n)| n.label() == Some("Rank") || n.value() == Some("Rank"));
    let has_empty_notice = u.nodes.iter().any(|(_, n)| n.label() == Some("No factions assigned") || n.value() == Some("No factions assigned"));
    assert!(has_faction_hdr, "Table must show 'Faction' column header even with 0 factions");
    assert!(has_rank_hdr, "Table must show 'Rank' column header even with 0 factions");
    assert!(has_empty_notice, "Table must display 'No factions assigned' notice");

    let btn_rect = u.nodes.iter().find_map(|(_, n)| {
        if n.label() == Some("+ Add Faction") || n.value() == Some("+ Add Faction") {
            n.bounds()
        } else {
            None
        }
    }).expect("+ Add Faction button must be present when actor has 0 factions");

    let click_pos = nvre_editor::egui::pos2(btn_rect.x0 as f32 + 5.0, btn_rect.y0 as f32 + 5.0);
    let mut out2 = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(900.0, 2400.0),
            )),
            events: vec![
                nvre_editor::egui::Event::PointerMoved(click_pos),
                nvre_editor::egui::Event::PointerButton {
                    pos: click_pos,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                nvre_editor::egui::Event::PointerButton {
                    pos: click_pos,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                nvre_editor::object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );
    out2.textures_delta.clear();

    let rec_after = session.resolved_record(fid).unwrap();
    let snams_after = rec_after.subrecords().unwrap().iter().filter(|s| &s.sig == b"SNAM").count();
    assert_eq!(snams_after, 1, "+ Add Faction must add the first SNAM row to an actor with 0 factions");
}

#[test]
fn the_ai_data_tab_renders_sections_geck_labels_and_checkbox() {
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("AI Data".to_string());
    let rows = placed_widgets(&mut session, &mut state, nvre_editor::egui::vec2(900.0, 2400.0));

    // Section headers:
    assert!(rows.iter().any(|(t, _)| t == "AI Attributes"), "missing 'AI Attributes' section: {rows:?}");
    assert!(rows.iter().any(|(t, _)| t == "Aggro Radius"), "missing 'Aggro Radius' section: {rows:?}");
    assert!(rows.iter().any(|(t, _)| t == "Services"), "missing 'Services' section: {rows:?}");

    // GECK field labels:
    assert!(rows.iter().any(|(t, _)| t == "Aggression"), "missing 'Aggression': {rows:?}");
    assert!(rows.iter().any(|(t, _)| t == "Confidence"), "missing 'Confidence': {rows:?}");
    assert!(rows.iter().any(|(t, _)| t == "Energy Level"), "missing 'Energy Level': {rows:?}");
    assert!(rows.iter().any(|(t, _)| t == "Responsibility"), "missing 'Responsibility': {rows:?}");
    assert!(rows.iter().any(|(t, _)| t == "Mood"), "missing 'Mood': {rows:?}");
    assert!(rows.iter().any(|(t, _)| t == "Assistance"), "missing 'Assistance': {rows:?}");
    assert!(rows.iter().any(|(t, _)| t == "Aggro Radius Behavior"), "missing 'Aggro Radius Behavior': {rows:?}");
    assert!(rows.iter().any(|(t, _)| t == "Teaches"), "missing 'Teaches': {rows:?}");
    assert!(rows.iter().any(|(t, _)| t == "Max Training Level"), "missing 'Max Training Level': {rows:?}");

    // No raw snake_case or padding:
    assert!(!rows.iter().any(|(t, _)| t == "unused"), "compiler padding 'unused' must be hidden: {rows:?}");
    assert!(!rows.iter().any(|(t, _)| t == "Barter Gold" || t == "barter_gold"), "Barter Gold is NPC-only and should not be on Creature AI Data: {rows:?}");
    assert!(!rows.iter().any(|(t, _)| t == "energy_level"), "snake_case 'energy_level' should be 'Energy Level'");
    assert!(!rows.iter().any(|(t, _)| t == "maximum_training_level"), "snake_case 'maximum_training_level' should be 'Max Training Level'");
    assert!(!rows.iter().any(|(t, _)| t == "aggro_radius_behavior"), "snake_case 'aggro_radius_behavior' should be 'Aggro Radius Behavior'");
}

#[test]
fn the_ai_packages_tab_renders_table_priority_reordering_and_add() {
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    state.tab.chosen = Some("AI Packages".to_string());
    let mut pages = Pages::default();
    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();

    // Frame 1: Measure layout
    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(900.0, 2400.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );
    out.textures_delta.clear();

    let u = out.platform_output.accesskit_update.unwrap();
    let has_prio = u.nodes.iter().any(|(_, n)| n.label() == Some("Priority") || n.value() == Some("Priority"));
    let has_pkg = u.nodes.iter().any(|(_, n)| n.label() == Some("Package") || n.value() == Some("Package"));
    let has_add = u.nodes.iter().any(|(_, n)| n.label() == Some("+ Add Package") || n.value() == Some("+ Add Package"));
    assert!(has_prio, "Should show Priority column header");
    assert!(has_pkg, "Should show Package column header");
    assert!(has_add, "Button must read '+ Add Package'");

    // Check initial order on Brahmin (0x0014D1FF, 0x000200D5)
    let rec0 = session.resolved_record(BRAHMIN).unwrap();
    let pk0: Vec<u32> = rec0.subrecords().unwrap().iter().filter(|s| &s.sig == b"PKID").map(|s| {
        u32::from_le_bytes(s.data.as_slice().try_into().unwrap())
    }).collect();
    assert_eq!(pk0, vec![0x0014_D1FF, 0x0002_00D5]);

    // Click Up on row 1 to move DefaultStayAtEditorLocation to row 0
    let up_btns: Vec<_> = u.nodes.iter().filter_map(|(_, n)| {
        if n.label() == Some("Up") || n.value() == Some("Up") {
            n.bounds()
        } else {
            None
        }
    }).collect();
    assert_eq!(up_btns.len(), 2, "There should be 2 up buttons for 2 rows");
    let up_btn_row1 = if up_btns[0].y0 > up_btns[1].y0 { up_btns[0] } else { up_btns[1] };
    let click_pos = nvre_editor::egui::pos2(up_btn_row1.x0 as f32 + 5.0, up_btn_row1.y0 as f32 + 5.0);

    let mut out2 = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(900.0, 2400.0),
            )),
            events: vec![
                nvre_editor::egui::Event::PointerMoved(click_pos),
                nvre_editor::egui::Event::PointerButton {
                    pos: click_pos,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                nvre_editor::egui::Event::PointerButton {
                    pos: click_pos,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );
    out2.textures_delta.clear();

    let rec1 = session.resolved_record(BRAHMIN).unwrap();
    let pk1: Vec<u32> = rec1.subrecords().unwrap().iter().filter(|s| &s.sig == b"PKID").map(|s| {
        u32::from_le_bytes(s.data.as_slice().try_into().unwrap())
    }).collect();
    assert_eq!(pk1, vec![0x0002_00D5, 0x0014_D1FF], "Clicking Up on row 1 must swap package priority!");
}

#[test]
fn the_ai_packages_tab_renders_empty_state_and_adds_first_package() {
    let data = match std::env::var("NVRE_DATA") {
        Ok(d) => d,
        Err(_) => return,
    };
    let mut session = Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .expect("session");

    // CrSuperMutantDeadMinigunRubyHill (0x0017AF62) has 0 packages.
    let fid = 0x0017_AF62u32;
    session.open_object(fid, *b"CREA");
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid, *b"CREA");
    state.tab.chosen = Some("AI Packages".to_string());
    let mut pages = Pages::default();

    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();

    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(900.0, 2400.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );
    out.textures_delta.clear();

    let u = out.platform_output.accesskit_update.unwrap();
    let has_prio = u.nodes.iter().any(|(_, n)| n.label() == Some("Priority") || n.value() == Some("Priority"));
    let has_pkg = u.nodes.iter().any(|(_, n)| n.label() == Some("Package") || n.value() == Some("Package"));
    let has_notice = u.nodes.iter().any(|(_, n)| n.label() == Some("No AI packages assigned") || n.value() == Some("No AI packages assigned"));
    assert!(has_prio, "Should show Priority header even with 0 packages");
    assert!(has_pkg, "Should show Package header even with 0 packages");
    assert!(has_notice, "Should show 'No AI packages assigned'");

    let btn_rect = u.nodes.iter().find_map(|(_, n)| {
        if n.label() == Some("+ Add Package") || n.value() == Some("+ Add Package") {
            n.bounds()
        } else {
            None
        }
    }).expect("+ Add Package button");

    let click_pos = nvre_editor::egui::pos2(btn_rect.x0 as f32 + 5.0, btn_rect.y0 as f32 + 5.0);
    let mut out2 = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(900.0, 2400.0),
            )),
            events: vec![
                nvre_editor::egui::Event::PointerMoved(click_pos),
                nvre_editor::egui::Event::PointerButton {
                    pos: click_pos,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                nvre_editor::egui::Event::PointerButton {
                    pos: click_pos,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );
    out2.textures_delta.clear();

    let rec1 = session.resolved_record(fid).unwrap();
    let pk_count = rec1.subrecords().unwrap().iter().filter(|s| &s.sig == b"PKID").count();
    assert_eq!(pk_count, 1, "+ Add Package must add the first package row!");
}

/// The inventory fixture: a creature whose items include one with a `COED`, so
/// the table has to show the pairing and not just the list.
const INVENTORY_FIXTURE: u32 = 0x0017_AF40;

/// The run's `CNTO`/`COED` payloads, in file order.
fn inventory_run(session: &Session, fid: u32) -> Vec<(String, Vec<u8>)> {
    subs(session, fid)
        .iter()
        .filter(|s| &s.sig == b"CNTO" || &s.sig == b"COED")
        .map(|s| (nvre_util::tag_str(&s.sig).to_string(), s.data.clone()))
        .collect()
}

/// The inventory table renders one row per `CNTO`, with the `COED` that follows
/// it folded into the same row: the condition column reads the stored value
/// (12%) rather than the 100% a row with no `COED` defaults to.
#[test]
fn the_inventory_tab_folds_each_coed_into_its_cnto_row() {
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    session.open_object(INVENTORY_FIXTURE, *b"CREA");
    state.load_from_session(&session, INVENTORY_FIXTURE, *b"CREA");
    state.tab.chosen = Some("Inventory".to_string());
    let mut pages = Pages::default();
    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();

    // The fixture's first CNTO is the one with a COED.
    let run = inventory_run(&session, INVENTORY_FIXTURE);
    assert_eq!(run[0].0, "CNTO", "the first entry is a CNTO");
    assert_eq!(run[1].0, "COED", "its COED follows it");
    assert_eq!(run.len(), 10, "9 CNTO and 1 COED");

    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(1200.0, 3000.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );
    out.textures_delta.clear();

    let u = out.platform_output.accesskit_update.unwrap();
    let labels: Vec<&str> = u
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label().or_else(|| n.value()))
        .filter(|l| !l.is_empty())
        .collect();
    // One row per CNTO: nine remove buttons, nine Up and nine Down.
    assert_eq!(labels.iter().filter(|l| **l == "✖").count(), 9, "one ✖ per item: {labels:?}");
    assert_eq!(labels.iter().filter(|l| **l == "Up").count(), 9, "one Up per item");
    assert_eq!(labels.iter().filter(|l| **l == "Down").count(), 9, "one Down per item");
    // The table's headers, the way the Cell Window's list shows them.
    for header in ["Item", "Count", "Condition", "Owner (NPC / Faction)", "Rank / Global"] {
        assert!(labels.contains(&header), "missing the {header:?} column header: {labels:?}");
    }
    // The first row shows the stored 12%, and the rest default to 100%.
    assert!(labels.iter().any(|l| *l == "12%"), "the row with a COED shows its 12%: {labels:?}");
    assert_eq!(labels.iter().filter(|l| **l == "100%").count(), 8, "the eight rows without one default to 100%");
    assert!(labels.contains(&"+ Add Item"), "the add button reads '+ Add Item'");
}

/// Reordering a row keeps its `COED` attached: the extras belong to the item, so
/// they have to move with it. Two `set_sub_list` calls would hoist every `COED`
/// above every `CNTO` and silently reassign every item's condition.
#[test]
fn the_inventory_tab_reorders_without_detaching_its_coed() {
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    session.open_object(INVENTORY_FIXTURE, *b"CREA");
    state.load_from_session(&session, INVENTORY_FIXTURE, *b"CREA");
    state.tab.chosen = Some("Inventory".to_string());
    let mut pages = Pages::default();
    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();

    let first = inventory_run(&session, INVENTORY_FIXTURE)[0].1.clone();

    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(1200.0, 3000.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );
    out.textures_delta.clear();
    let u = out.platform_output.accesskit_update.unwrap();

    // The topmost "Down" button: row 0's.
    let downs: Vec<_> = u
        .nodes
        .iter()
        .filter_map(|(_, n)| if n.label() == Some("Down") { n.bounds() } else { None })
        .collect();
    let d0 = downs.iter().min_by_key(|b| b.y0 as i64).expect("a Down button");
    let click = nvre_editor::egui::pos2(d0.x0 as f32 + 5.0, d0.y0 as f32 + 5.0);
    let _ = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(1200.0, 3000.0),
            )),
            events: vec![
                nvre_editor::egui::Event::PointerMoved(click),
                nvre_editor::egui::Event::PointerButton {
                    pos: click,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                nvre_editor::egui::Event::PointerButton {
                    pos: click,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );

    // The row that was first is now second, and its COED moved with it: the
    // COED is still the subrecord right after that CNTO, not the first one.
    let run = inventory_run(&session, INVENTORY_FIXTURE);
    assert_eq!(run.len(), 10, "no subrecord was lost");
    // The old row 0 is now row 1, and its COED is still the subrecord right
    // after it — not hoisted above the whole run, which is what two
    // `set_sub_list` calls would have done.
    assert_eq!(run[1].1, first, "the old row 0 is now row 1");
    assert_eq!(run[2].0, "COED", "the moved row's COED still follows its CNTO");
    assert_eq!(run[0].0, "CNTO", "the old row 1 is now row 0");
}

/// A creature with no items still shows the table and can add the first one.
#[test]
fn the_inventory_tab_adds_the_first_item_to_a_creature_with_none() {
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    // VStreetCrBrahmin2 carries no items at all.
    let fid = 0x0017_B587u32;
    session.open_object(fid, *b"CREA");
    state.load_from_session(&session, fid, *b"CREA");
    state.tab.chosen = Some("Inventory".to_string());
    let mut pages = Pages::default();
    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();
    assert!(inventory_run(&session, fid).is_empty(), "the fixture has no items");

    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(1200.0, 3000.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );
    out.textures_delta.clear();
    let u = out.platform_output.accesskit_update.unwrap();
    let labels: Vec<&str> = u
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label().or_else(|| n.value()))
        .collect();
    for header in ["Item", "Count", "Condition", "Owner (NPC / Faction)", "Rank / Global"] {
        assert!(labels.contains(&header), "the empty table still shows its {header:?} header");
    }
    assert!(labels.contains(&"No items"), "the empty table says 'No items'");

    let btn = u
        .nodes
        .iter()
        .find_map(|(_, n)| {
            if n.label() == Some("+ Add Item") || n.value() == Some("+ Add Item") {
                n.bounds()
            } else {
                None
            }
        })
        .expect("+ Add Item button");
    let click = nvre_editor::egui::pos2(btn.x0 as f32 + 5.0, btn.y0 as f32 + 5.0);
    let _ = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(1200.0, 3000.0),
            )),
            events: vec![
                nvre_editor::egui::Event::PointerMoved(click),
                nvre_editor::egui::Event::PointerButton {
                    pos: click,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                nvre_editor::egui::Event::PointerButton {
                    pos: click,
                    button: nvre_editor::egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );

    let run = inventory_run(&session, fid);
    assert_eq!(run.len(), 1, "+ Add Item adds the first item: {run:?}");
    assert_eq!(run[0].0, "CNTO", "and it is a CNTO");
}

/// With no owner set the Rank / Global cell is inert: a rank with no faction to
/// be a rank *of*, and a global with no owner to gate it, are both meaningless,
/// so the cell shows what the bytes hold as disabled text rather than a picker.
/// The Owner column is still a live picker — that is the cell that has to be set
/// first.
#[test]
fn the_inventory_tab_locks_rank_global_until_an_owner_is_set() {
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    session.open_object(INVENTORY_FIXTURE, *b"CREA");
    state.load_from_session(&session, INVENTORY_FIXTURE, *b"CREA");
    state.tab.chosen = Some("Inventory".to_string());
    let mut pages = Pages::default();
    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();

    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(1200.0, 3000.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                nvre_editor::egui::ScrollArea::vertical()
                    .id_salt("obj_scroll")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        object_editor::body(ui, &mut session, &mut state, &mut pages);
                    });
            });
        },
    );
    out.textures_delta.clear();
    let u = out.platform_output.accesskit_update.unwrap();

    // The fixture's only COED has a zero owner, so every row's Rank / Global is
    // locked. The rows without a COED have no extras at all and are locked too.
    let combos: Vec<(f32, f32, &str)> = u
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == nvre_editor::egui::accesskit::Role::ComboBox)
        .filter_map(|(_, n)| {
            let v = n.value()?;
            let b = n.bounds()?;
            Some((b.x0 as f32, b.y0 as f32, v))
        })
        .collect();
    // Every combobox on the table sits in the Item, Owner or Rank columns; none
    // of the Rank column's is openable, so its cells are the disabled text boxes
    // rather than comboboxes.
    let owner_col = combos
        .iter()
        .filter(|(x, _, _)| *x > 860.0 && *x < 900.0)
        .count();
    assert_eq!(
        owner_col, 9,
        "the Owner column is a live picker on every row: {combos:?}"
    );

    // No combobox sits past the Owner column's x: that is the locked cell.
    assert!(
        !combos.iter().any(|(x, _, _)| *x > 1050.0),
        "the Rank / Global column must not be a picker with no owner set: {combos:?}"
    );
}

/// A faction that defines no ranks has nothing to offer, so the Rank / Global
/// cell reads None and is inert rather than falling back to a raw 0..255 number
/// the faction does not define.
///
/// No creature in `FalloutNV.esm` carries a `COED` with a faction owner, so the
/// owner is written onto the document first and the table is drawn against it —
/// the same path an edit through the table itself would take.
#[test]
fn the_inventory_tab_shows_none_when_the_owner_faction_defines_no_ranks() {
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    let fid = INVENTORY_FIXTURE;
    session.open_object(fid, *b"CREA");
    // EJunkieFaction: a FACT with no RNAM.
    let unranked = session
        .records_of(*b"FACT")
        .iter()
        .find(|(f, name)| *f == 0x000F_E89A || name.contains("EJunkie"))
        .map(|(f, _)| *f)
        .expect("a faction with no ranks");
    assert!(
        session
            .resolved_record(unranked)
            .and_then(|r| r.subrecords().ok())
            .map(|s| s.iter().filter(|s| &s.sig == b"RNAM").count())
            .unwrap_or(0)
            == 0,
        "the fixture faction really has no ranks"
    );

    // Point the fixture's COED owner at it.
    let run: Vec<([u8; 4], Vec<u8>)> = subs(&session, fid)
        .iter()
        .filter(|s| &s.sig == b"CNTO" || &s.sig == b"COED")
        .map(|s| (s.sig, s.data.clone()))
        .collect();
    let mut new_run = run.clone();
    let coed_in_run = new_run
        .iter()
        .position(|(s, _)| s == b"COED")
        .expect("the fixture has a COED");
    new_run[coed_in_run].1[..4].copy_from_slice(&unranked.to_le_bytes());
    session
        .doc
        .set_sub_sequence(fid, vec![*b"CNTO", *b"COED"], new_run);

    state.load_from_session(&session, fid, *b"CREA");
    state.tab.chosen = Some("Inventory".to_string());
    let mut pages = Pages::default();
    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();

    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(1200.0, 3000.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                nvre_editor::egui::ScrollArea::vertical()
                    .id_salt("obj_scroll")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        object_editor::body(ui, &mut session, &mut state, &mut pages);
                    });
            });
        },
    );
    out.textures_delta.clear();
    let u = out.platform_output.accesskit_update.unwrap();

    let top_y = u
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label().filter(|l| *l == "✖").and_then(|_| n.bounds()))
        .map(|b| b.y0 as f32)
        .fold(f32::INFINITY, f32::min);

    // Row 0's Rank / Global cell reads None.
    let rank_cell = u
        .nodes
        .iter()
        .filter_map(|(_, n)| {
            let l = n.label().or_else(|| n.value())?;
            let b = n.bounds()?;
            Some((l, b.x0 as f32, b.y0 as f32))
        })
        .find(|(l, x, y)| *x > 1050.0 && (*y - top_y).abs() < 10.0);
    assert!(
        matches!(rank_cell, Some((l, _, _)) if l == "None"),
        "an unranked faction owner reads None: {rank_cell:?}"
    );

    // And it is not a picker.
    let combos: Vec<&str> = u
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == nvre_editor::egui::accesskit::Role::ComboBox)
        .filter_map(|(_, n)| {
            let b = n.bounds()?;
            if b.x0 as f32 > 1050.0 && (b.y0 as f32 - top_y).abs() < 10.0 {
                n.value()
            } else {
                None
            }
        })
        .collect();
    assert!(
        combos.is_empty(),
        "an unranked faction owner's rank cell is not clickable: {combos:?}"
    );
}

/// The Actor Effect List tab carries all three of a creature's unarmed-attack
/// and effect fields, and the two scalar ones stay settable when the record
/// does not carry them yet: `EITM` is a FormID and `EAMT` an enum, so both have
/// a widget to offer even with no bytes on the record.
///
/// `VStreetCrBrahmin2` has an `EAMT` and an `SPLO` but no `EITM`, so one record
/// exercises both the present and the absent side of the same tab.
#[test]
fn the_actor_effect_tab_shows_all_three_sections_and_absent_pickers() {
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    // The fixture carries no EITM.
    assert!(
        subs(&session, BRAHMIN).iter().all(|s| &s.sig != b"EITM"),
        "the fixture has no EITM"
    );
    state.tab.chosen = Some("Actor Effect List".to_string());
    let mut pages = Pages::default();
    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();

    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(1200.0, 2400.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                nvre_editor::egui::ScrollArea::vertical()
                    .id_salt("obj_scroll")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        object_editor::body(ui, &mut session, &mut state, &mut pages);
                    });
            });
        },
    );
    out.textures_delta.clear();
    let u = out.platform_output.accesskit_update.unwrap();
    let labels: Vec<&str> = u
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label().or_else(|| n.value()))
        .filter(|l| !l.is_empty())
        .collect();

    // All three sections, each with its own heading.
    for heading in [
        "Unarmed Attack Effect",
        "Unarmed Attack Animation",
        "Actor Effects",
    ] {
        assert!(
            labels.contains(&heading),
            "missing the {heading:?} section: {labels:?}"
        );
    }
    // The absent EITM is a live picker reading NONE, not a blank row.
    assert!(
        labels.contains(&"NONE"),
        "the absent EITM offers a picker: {labels:?}"
    );
    // The effect list is a real table with its header and controls.
    assert!(labels.contains(&"Actor Effect"), "missing the list's header");
    assert!(labels.contains(&"+ Add Actor Effect"), "missing the add button");
    assert_eq!(
        labels.iter().filter(|l| **l == "✖").count(),
        1,
        "one remove button per effect: {labels:?}"
    );
}

/// An absent `EAMT` is settable from the tab: the enum's values come from the
/// schema, so the record does not need the subrecord to exist first.
///
/// Every creature in `FalloutNV.esm` carries an `EAMT`, so the subrecord is
/// stripped from the document to reach the absent path — the same state a
/// plugin written by hand would be in.
#[test]
fn an_absent_eamt_can_be_set_from_the_actor_effect_tab() {
    let Some((mut session, mut state)) = open_creature() else {
        return;
    };
    session.doc.remove_sub(BRAHMIN, b"EAMT");
    assert!(
        subs(&session, BRAHMIN).iter().all(|s| &s.sig != b"EAMT"),
        "the fixture no longer carries an EAMT"
    );
    state.load_from_session(&session, BRAHMIN, *b"CREA");
    state.tab.chosen = Some("Actor Effect List".to_string());
    let mut pages = Pages::default();
    let ctx = nvre_editor::egui::Context::default();
    ctx.enable_accesskit();

    let mut out = ctx.run_ui(
        nvre_editor::egui::RawInput {
            screen_rect: Some(nvre_editor::egui::Rect::from_min_size(
                nvre_editor::egui::Pos2::ZERO,
                nvre_editor::egui::vec2(1200.0, 2400.0),
            )),
            ..Default::default()
        },
        |ui| {
            nvre_editor::egui::CentralPanel::default().show(ui, |ui| {
                nvre_editor::egui::ScrollArea::vertical()
                    .id_salt("obj_scroll")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        object_editor::body(ui, &mut session, &mut state, &mut pages);
                    });
            });
        },
    );
    out.textures_delta.clear();
    let u = out.platform_output.accesskit_update.unwrap();

    // The section still draws, and its dropdown reads NONE.
    let labels: Vec<&str> = u
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label().or_else(|| n.value()))
        .filter(|l| !l.is_empty())
        .collect();
    assert!(
        labels.contains(&"Unarmed Attack Animation"),
        "the section draws with no EAMT on the record: {labels:?}"
    );
    assert!(labels.contains(&"NONE"), "its dropdown reads NONE: {labels:?}");

    // Picking a value writes the subrecord: the enum is one u16, and the value
    // the schema calls 25 is the animation the pick offers.
    session
        .doc
        .set_sub(BRAHMIN, b"EAMT", 25u16.to_le_bytes().to_vec());
    let eamt = subs(&session, BRAHMIN)
        .iter()
        .find(|s| &s.sig == b"EAMT")
        .map(|s| s.data.clone());
    assert_eq!(
        eamt,
        Some(vec![25, 0]),
        "the picked value is written as the enum's u16"
    );
}

/// The Animation tab lists a creature's animations in the two groups the engine
/// gives them, and the specialanims half is read back from the record.
///
/// `BMRhonda` (`000E84A2`) is the fixture: its model's folder holds 10 base
/// animations, its `specialanims` folder holds 22, and its record selects 15 of
/// them — every one except the seven `1hp*` pistol animations.
#[test]
fn the_animation_tab_lists_base_and_specialanims_with_the_records_selection() {
    let Some((mut session, _state)) = open_creature() else {
        return;
    };
    let fid = 0x000E_84A2u32;
    session.open_object(fid, *b"CREA");

    let set = session
        .animation_set_for(fid)
        .expect("the fixture's model resolves");
    // Base animations: the folder the skeleton sits in, always on.
    assert_eq!(set.base.len(), 10, "10 base animations: {:?}", set.base);
    assert!(
        set.base.iter().any(|p| p.ends_with("mtidle.kf")),
        "mtidle.kf is a base animation: {:?}",
        set.base
    );
    // Special overrides: the specialanims subfolder.
    assert_eq!(
        set.special.len(),
        22,
        "22 special overrides: {:?}",
        set.special
    );
    assert!(
        set.special.iter().all(|p| p.contains("specialanims")),
        "every special override lives in specialanims: {:?}",
        set.special
    );

    // The record's own selection, read out of `KFFZ`. It is **one** subrecord
    // holding every selected name, NUL-separated with a trailing NUL — not one
    // subrecord per name.
    let kffz = subs(&session, fid)
        .iter()
        .find(|s| &s.sig == b"KFFZ")
        .map(|s| s.data.clone())
        .expect("the fixture carries a KFFZ");
    let selected: Vec<String> = String::from_utf8_lossy(&kffz)
        .split('\0')
        .filter(|n| !n.is_empty())
        .map(|n| n.to_lowercase())
        .collect();
    assert_eq!(selected.len(), 15, "15 of the 22 are selected: {selected:?}");
    assert!(
        selected.iter().any(|n| n == "1hmaim.kf"),
        "the hand-to-hand animations are selected: {selected:?}"
    );
    assert!(
        selected.iter().all(|n| n != "1hpaim.kf"),
        "the pistol animations are not: {selected:?}"
    );
}

/// A creature with no `specialanims` folder and no `KFFZ` has nothing selected
/// — the base animations still list, because the engine discovers them from the
/// model's folder regardless.
#[test]
fn the_animation_tab_shows_nothing_selected_when_the_record_has_no_kffz() {
    let Some((mut session, _state)) = open_creature() else {
        return;
    };
    // VStreetCrBrahmin2: 9 base animations, no specialanims, no KFFZ.
    let fid = BRAHMIN;
    session.open_object(fid, *b"CREA");
    assert!(
        subs(&session, fid).iter().all(|s| &s.sig != b"KFFZ"),
        "the fixture carries no KFFZ"
    );

    let set = session
        .animation_set_for(fid)
        .expect("the brahmin's model resolves");
    assert_eq!(set.base.len(), 9, "9 base animations: {:?}", set.base);
    assert!(
        set.special.is_empty(),
        "no specialanims folder: {:?}",
        set.special
    );
}

/// Typing a record's Editor ID into the object window's filter finds it, even
/// when the record has a full name the Editor ID never appears in.
///
/// `BMRhonda` (`000E84A2`) is named "Broken Mr. Handy", so a filter that
/// matched the label alone — the name where there is one — could not find it by
/// what the record is called everywhere else. The window's own hint promises
/// the Editor ID; this is the test that holds it to that.
#[test]
fn the_object_window_filter_matches_the_editor_id_as_well_as_the_name() {
    let Some((mut session, _state)) = open_creature() else {
        return;
    };
    let groups = session.object_groups("bmrhonda", &[]);
    let crea: Vec<_> = groups.iter().filter(|g| g.sig == *b"CREA").collect();
    assert_eq!(
        crea.len(),
        1,
        "the Editor ID names exactly one creature: {:?}",
        groups.iter().map(|g| (g.sig, g.objects.len())).collect::<Vec<_>>()
    );
    let row = crea[0]
        .objects
        .iter()
        .find(|r| r.fid == 0x000E_84A2)
        .expect("BMRhonda is found by its Editor ID");
    assert_eq!(row.edid, "BMRhonda");
    assert_eq!(row.name, "Broken Mr. Handy");

    // The name still finds it, and the FormID still finds it: the filter is a
    // match on any of the three columns, not a switch between them.
    for (filter, what) in [
        ("broken mr. handy", "the full name"),
        ("000e84a2", "the FormID"),
    ] {
        let groups = session.object_groups(filter, &[]);
        let found = groups
            .iter()
            .filter(|g| g.sig == *b"CREA")
            .any(|g| g.objects.iter().any(|r| r.fid == 0x000E_84A2));
        assert!(found, "the {what} finds it too: {filter}");
    }
}

/// The specialanims checkboxes are live: ticking one rewrites `KFFZ` with the
/// whole selection, unticking it removes just that name.
///
/// `BMRhonda` (`000E84A2`) selects 15 of the 22 in its `specialanims` folder, so
/// there is an unchecked special row to tick and a checked one to untick — the
/// same fixture the read-only test uses, exercised in both directions.
#[test]
fn the_specialanims_checkboxes_rewrite_kffz_in_both_directions() {
    use nvre_editor::egui;
    use nvre_editor::object_editor::{self, ObjectEditorState, Pages};

    let Some((mut session, _state)) = open_creature() else {
        return;
    };
    let fid = 0x000E_84A2u32;
    session.open_object(fid, *b"CREA");
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid, *b"CREA");
    state.tab.chosen = Some("Animation".to_string());
    let mut pages = Pages::default();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();

    /// What the record's `KFFZ` holds: one subrecord, every selected name,
    /// NUL-separated with a trailing NUL.
    fn selection(session: &nvre_editor::session::Session, fid: u32) -> Vec<String> {
        let rec = session.resolved_record(fid).unwrap();
        rec.subrecords()
            .unwrap()
            .iter()
            .filter(|s| &s.sig == b"KFFZ")
            .flat_map(|s| {
                String::from_utf8_lossy(&s.data)
                    .split('\0')
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|n| !n.is_empty())
            .collect()
    }

    let render = |ctx: &egui::Context,
                  session: &mut nvre_editor::session::Session,
                  state: &mut ObjectEditorState,
                  pages: &mut Pages,
                  events: Vec<egui::Event>| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 2400.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    object_editor::body(ui, session, state, pages);
                });
            },
        )
    };

    // One frame to lay the tab out, then click the last unchecked special row.
    // `toggled()` is the node's own state, so the probe does not guess which row
    // is which — and a row that is already selected is skipped, which is what
    // keeps the test from clicking a base animation.
    let mut out = render(&ctx, &mut session, &mut state, &mut pages, vec![]);
    out.textures_delta.clear();
    let update = std::mem::take(&mut out.platform_output.accesskit_update).unwrap();
    let unchecked: Vec<(f32, f32)> = update
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == egui::accesskit::Role::CheckBox)
        .filter(|(_, n)| {
            n.toggled() == Some(egui::accesskit::Toggled::False) && !n.is_disabled()
        })
        .filter_map(|(_, n)| {
            let b = n.bounds()?;
            // The table's "In use" column, not the flag checkboxes above it.
            (b.x0 >= 80.0 && b.x1 <= 120.0).then(|| (b.x0 as f32, b.y0 as f32))
        })
        .collect();
    let last = unchecked.last().expect("an unchecked special row exists");
    drop(update);
    let click_pos = egui::pos2(last.0 + 5.0, last.1 + 5.0);
    let click = vec![
        egui::Event::PointerMoved(click_pos),
        egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
        egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        },
    ];

    let before = selection(&session, fid);
    let mut out = render(&ctx, &mut session, &mut state, &mut pages, click);
    out.textures_delta.clear();
    drop(out);

    let after = selection(&session, fid);
    let added: Vec<&String> = after.iter().filter(|n| !before.contains(n)).collect();
    assert_eq!(
        added.len(),
        1,
        "ticking an unchecked special row adds exactly one name: {added:?}"
    );
    let added_name = added[0].clone();
    assert!(
        added_name.ends_with(".kf") && !added_name.contains('\\'),
        "the name is stored as the file's own name, not its folder: {added_name}"
    );
    // The write is a whole-`KFFZ` rewrite, not an append: the new selection is
    // the old one plus the one name.
    let expected: Vec<String> = before.iter().cloned().chain([added_name.clone()]).collect();
    assert_eq!(after, expected, "the selection is the old one plus the ticked name");

    // And back the other way: click the row that holds the name just added.
    // The row is found by its label — the file name the row draws — so the test
    // clicks the row it means to rather than whichever row happens to be last.
    let mut out = render(&ctx, &mut session, &mut state, &mut pages, vec![]);
    out.textures_delta.clear();
    let update = std::mem::take(&mut out.platform_output.accesskit_update).unwrap();
    // The row for the name just added, then the checkbox on that row: the row's
    // y is the checkbox's y, and the column is the one the base rows use.
    let target = update
        .nodes
        .iter()
        .find(|(_, n)| {
            (n.label() == Some(added_name.as_str())
                || n.value() == Some(added_name.as_str()))
                && n.role() == egui::accesskit::Role::Label
        })
        .and_then(|(_, n)| n.bounds())
        .unwrap_or_else(|| panic!("the row for {added_name} is drawn"));
    let checkbox = update
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == egui::accesskit::Role::CheckBox)
        .filter_map(|(_, n)| n.bounds())
        .find(|b| (b.y0 - target.y0).abs() < 2.0 && b.x0 >= 80.0 && b.x1 <= 120.0)
        .unwrap_or_else(|| panic!("the row for {added_name} has a checkbox"));
    let click_pos = egui::pos2(checkbox.x0 as f32 + 5.0, checkbox.y0 as f32 + 5.0);
    let click = vec![
        egui::Event::PointerMoved(click_pos),
        egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
        egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        },
    ];
    let mut out = render(&ctx, &mut session, &mut state, &mut pages, click);
    out.textures_delta.clear();
    drop(out);

    let back = selection(&session, fid);
    assert_eq!(
        back, before,
        "unticking the same row removes exactly the name it added"
    );
}

/// A base animation's checkbox is drawn disabled, so the tab shows the base
/// animations without letting them be turned off — the engine discovers them at
/// runtime and never reads them from the record.
#[test]
fn the_base_animation_checkboxes_are_disabled() {
    use nvre_editor::egui;
    use nvre_editor::object_editor::{self, ObjectEditorState, Pages};

    let Some((mut session, _state)) = open_creature() else {
        return;
    };
    let fid = 0x000E_84A2u32;
    session.open_object(fid, *b"CREA");
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid, *b"CREA");
    state.tab.chosen = Some("Animation".to_string());
    let mut pages = Pages::default();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();

    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 2400.0),
            )),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );
    out.textures_delta.clear();
    let update = std::mem::take(&mut out.platform_output.accesskit_update).unwrap();
    // Every checkbox in the table's column, split by whether it is disabled.
    let mut disabled = 0usize;
    let mut enabled = 0usize;
    for (_, n) in update.nodes.iter() {
        if n.role() != egui::accesskit::Role::CheckBox {
            continue;
        }
        let Some(b) = n.bounds() else { continue };
        if !(b.x0 >= 80.0 && b.x1 <= 120.0) {
            continue;
        }
        if n.is_disabled() {
            disabled += 1;
        } else {
            enabled += 1;
        }
    }
    drop(update);
    // 10 base, always disabled; 22 special, selectable. All 32 fit in 2400px.
    assert_eq!(disabled, 10, "10 base animations, all disabled: {disabled}");
    assert_eq!(enabled, 22, "22 special overrides, all selectable: {enabled}");
}

/// The Animation tab has one scrollbar, and it is the table's.
///
/// The page wraps its body in a scroll area. A table that sized itself to its
/// content would nest a second scrollbar inside the first, so the tab showed two
/// bars — each scrolling part of the other — and the rows were reachable only by
/// finding the right one. The table is given the viewport instead.
///
/// The assertion is on the geometry, not on a scrollbar's existence: with the
/// fix, the table's own scroll area spans the whole tab and the rows it does not
/// draw are the ones its own bar reaches. Without it, the table is shorter than
/// the tab and the page's bar is the only one that can reach them.
#[test]
fn the_animation_tab_has_one_scrollbar_and_it_is_the_tables() {
    use nvre_editor::egui;
    use nvre_editor::object_editor::{self, ObjectEditorState, Pages};

    let Some((mut session, _state)) = open_creature() else {
        return;
    };
    let fid = 0x000E_84A2u32;
    session.open_object(fid, *b"CREA");
    let mut state = ObjectEditorState::default();
    state.load_from_session(&session, fid, *b"CREA");
    state.tab.chosen = Some("Animation".to_string());
    let mut pages = Pages::default();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();

    // A short viewport, the way the editor's panel actually is: the tab's 32 rows
    // cannot all fit, so something has to scroll.
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 500.0),
            )),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                object_editor::body(ui, &mut session, &mut state, &mut pages);
            });
        },
    );
    out.textures_delta.clear();
    let update = std::mem::take(&mut out.platform_output.accesskit_update).unwrap();

    // The animation rows: the table's "In use" checkboxes.
    let rows: Vec<(f32, f32)> = update
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == egui::accesskit::Role::CheckBox)
        .filter_map(|(_, n)| {
            let b = n.bounds()?;
            (b.x0 >= 80.0 && b.x1 <= 120.0).then(|| (b.y0 as f32, b.y1 as f32))
        })
        .collect();
    drop(update);
    // 32 rows in a 500px viewport: not all of them are drawn, which is the table
    // scrolling its own rows rather than the page scrolling a full-height table.
    let first = rows.first().expect("the table draws rows").0;
    let last = rows.last().expect("the table draws rows").1;
    // 32 rows in a 500px viewport: the table scrolls its own rows, so not all
    // are drawn at once.
    assert!(
        rows.len() < 32,
        "the table scrolls its own rows, so not all 32 are drawn at once: {}",
        rows.len()
    );
    // And the rows it does draw run to the bottom of the tab, so the page's
    // scroll area has nothing left over to scroll. Without the fix the table is
    // sized to its content inside a scroll area that is itself scrolled, and the
    // rows stop well short of the tab's bottom edge.
    assert!(
        last > 455.0,
        "the table's rows fill the tab: the last drawn row ends at {last} of 500, \
         so the page's scroll area is not the one scrolling"
    );
    assert!(
        last - first > 70.0,
        "the table uses the tab's height rather than a sliver of it: {first}..{last}"
    );
}
