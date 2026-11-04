//! A section can name a widget, and a widget that sections needed: repeated rows.
//!
//! The generic grid draws one row per field. That covers most of a record, but not
//! the subrecords a record repeats — a leveled list's entries, a container's
//! contents, a spell list — where the editing a modder wants is "add a row" and
//! "remove this row", not "change a number".
//!
//! A section names the widget: `{"title": "Entries", "subs": ["LVLO"], "widget":
//! "repeat"}`. The name is looked up in the editor's registry, and a name the
//! build does not have is reported on the page — a typo has to be visible, not
//! silently fall back to the default and look like it worked.
//!
//! What these tests hold is the mechanism: the schema carries the name through
//! layers, and a section's widget does not leak onto its neighbours.

use nvre_editor::game::GameProfile;
use nvre_editor::panels::widget_names;

#[test]
fn a_section_carries_its_widget_through_the_layer_chain() {
    // The name is data, so it has to survive `base -> family -> version` the same
    // way the sections do. A layer that restated the record's page without the
    // widget would silently drop it.
    let mut schema = nvre_esm::Schema::new();
    schema
        .merge_str(
            r#"{
              "format": "fnv.schema/1",
              "object_window": {"categories": {"LVLI": {"group": "Items", "sections": [
                {"title": "Entries", "subs": ["LVLO"], "widget": "repeat"},
                {"title": "Settings", "subs": ["DATA"]}
              ]}}}
            }"#,
        )
        .expect("the schema loads");
    let sections = schema.page_sections("LVLI");
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].widget.as_deref(), Some("repeat"));
    assert_eq!(
        sections[1].widget, None,
        "a widget belongs to its own section, not to the page"
    );
}

#[test]
fn a_widget_named_by_a_section_is_one_the_editor_registers() {
    // Every widget a game's table names must exist in this build, or the page
    // shows an error where a modder expected an editor. This is the assertion
    // that fails the day someone writes `"widget": "rows"` meaning `"repeat"`.
    let known = widget_names();
    let mut named = Vec::new();
    for id in ["fnv", "fo3", "skyrim", "skyrimse", "fo4"] {
        let profile = GameProfile::by_id(id).expect("a known game");
        let schema = profile.load_schema().expect("the schema loads");
        for sig in schema.object_window.categories.keys() {
            for section in schema.page_sections(sig) {
                if let Some(w) = &section.widget {
                    assert!(
                        known.contains(&w.as_str()),
                        "{id} {sig} names widget {w:?}, which this build does not register \
                         (known: {known:?})"
                    );
                    named.push((id, sig.clone(), w.clone()));
                }
            }
        }
    }
    eprintln!("widgets named by schemas: {named:?}");
}

#[test]
fn the_repeat_widget_is_registered() {
    // The widget the 18 surviving hand-written pages need: they exist because a
    // repeated list needs rows added and removed, which the grid cannot do.
    assert!(
        widget_names().contains(&"repeat"),
        "the repeat widget is what makes a list page data rather than code"
    );
}

#[test]
fn a_widget_that_is_not_a_string_is_refused() {
    // `"widget": 3` is a typo, and a typo has to fail where it is written rather
    // than render as something plausible.
    let mut schema = nvre_esm::Schema::new();
    let err = schema
        .merge_str(
            r#"{
              "format": "fnv.schema/1",
              "object_window": {"categories": {"LVLI": {"group": "Items", "sections": [
                {"title": "Entries", "subs": ["LVLO"], "widget": 3}
              ]}}}
            }"#,
        )
        .expect_err("a non-string widget is refused");
    assert!(err.contains("widget"), "{err}");
}
