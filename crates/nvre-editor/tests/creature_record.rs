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
