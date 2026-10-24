//! The TES4 header's author (`CNAM`) and description (`SNAM`).
//!
//! GECK's Create dialog has no author or description field: its plugin is always
//! `CNAM DEFAULT` and has no `SNAM` at all, which is why
//! `testfiles/fonv_geck_author_desc.esm` is byte-identical to
//! `testfiles/fonv_geck_empty.esm`. So there is no GECK file to reproduce here —
//! these assert the format rules instead, which are:
//!
//! - the order is `HEDR CNAM [SNAM] MAST/DATA [ONAM]` [measured over the FNV
//!   Data folder: 12 plugins are `HEDR CNAM MAST…`, one is
//!   `HEDR CNAM SNAM MAST…` — `nvre_add_weap.esm`];
//! - both are cstrings capped at 512 bytes including the terminator
//!   [documented: fopdoc's TES4 page];
//! - an empty description is no `SNAM`, not an empty one.

use nvre_edit::save::{write_plugin, MasterList};
use nvre_edit::Document;
use nvre_esm::Plugin;

/// A blank plugin with an author and description, read back by NVRE's reader.
fn create(author: Option<&str>, description: Option<&str>) -> Document {
    Document::create_empty(
        "Authored.esp",
        false,
        &["FalloutNV.esm".to_string()],
        author,
        description,
    )
    .unwrap()
}

/// The header's subrecord signatures, in file order.
fn header_sigs(doc: &Document) -> Vec<String> {
    doc.plugin()
        .nodes
        .first()
        .map_or_else(Vec::new, |n| match n {
            nvre_esm::raw::Node::Record(r) => r
                .subrecords()
                .unwrap()
                .iter()
                .map(|s| s.sig_str())
                .collect(),
            _ => Vec::new(),
        })
}

/// An author and a description both reach the header, in the format's order.
#[test]
fn the_author_and_description_are_written_in_the_formats_order() {
    let doc = create(Some("A Mod Author"), Some("What this mod does"));
    assert_eq!(
        header_sigs(&doc),
        vec!["HEDR", "CNAM", "SNAM", "MAST", "DATA"],
        "the order is HEDR CNAM SNAM MAST/DATA, as fopdoc documents and every installed plugin uses"
    );

    let header = &doc.plugin().header;
    assert_eq!(header.author.as_deref(), Some("A Mod Author"));
    assert_eq!(header.description.as_deref(), Some("What this mod does"));
}

/// An author with no description writes no `SNAM` — GECK's own empty plugin has
/// none, so an empty description must not become an empty subrecord.
#[test]
fn no_description_writes_no_snam() {
    let doc = create(Some("A Mod Author"), None);
    assert_eq!(header_sigs(&doc), vec!["HEDR", "CNAM", "MAST", "DATA"]);

    // And the same when the description is an empty string rather than absent.
    let empty = create(Some("A Mod Author"), Some(""));
    assert_eq!(
        header_sigs(&empty),
        vec!["HEDR", "CNAM", "MAST", "DATA"],
        "an empty description is no SNAM, not an empty one"
    );
    assert_eq!(empty.plugin().header.description, None);
}

/// With neither, the header is GECK's own: `CNAM DEFAULT`, no `SNAM`.
#[test]
fn no_author_falls_back_to_gecks_default() {
    let doc = create(None, None);
    assert_eq!(header_sigs(&doc), vec!["HEDR", "CNAM", "MAST", "DATA"]);
    assert_eq!(doc.plugin().header.author.as_deref(), Some("DEFAULT"));
    assert_eq!(doc.plugin().header.description, None);
}

/// Both are cstrings capped at 512 bytes including the terminator, so a longer
/// value is cut rather than written in a form the game cannot read.
#[test]
fn over_long_text_is_cut_to_the_formats_limit() {
    let long_author = "A".repeat(2000);
    let long_description = "B".repeat(2000);
    let doc = create(Some(&long_author), Some(&long_description));

    let header = &doc.plugin().header;
    let author = header.author.as_deref().expect("an author");
    let description = header.description.as_deref().expect("a description");
    assert_eq!(
        author.len(),
        nvre_edit::document::HEADER_TEXT_MAX - 1,
        "the author must be cut to 511 bytes, leaving room for the terminator"
    );
    assert_eq!(
        description.len(),
        nvre_edit::document::HEADER_TEXT_MAX - 1,
        "the description must be cut to 511 bytes"
    );

    // And the subrecord really is within the cap, terminator included.
    let subs: Vec<nvre_esm::Subrecord> = doc
        .plugin()
        .nodes
        .first()
        .and_then(|n| match n {
            nvre_esm::raw::Node::Record(r) => Some(r.subrecords().unwrap()),
            _ => None,
        })
        .expect("a TES4 record");
    for sig in [*b"CNAM", *b"SNAM"] {
        let s = subs
            .iter()
            .find(|s| s.sig == sig)
            .unwrap_or_else(|| panic!("{} present", String::from_utf8_lossy(&sig)));
        assert_eq!(
            s.data.len(),
            nvre_edit::document::HEADER_TEXT_MAX,
            "{} must be 512 bytes including its NUL terminator",
            String::from_utf8_lossy(&sig)
        );
        assert_eq!(s.data.last(), Some(&0), "and it must be NUL-terminated");
    }
}

/// A description the caller sets on the save replaces the header's own, and
/// keeping the source's is the default.
///
/// The failure this guards is the one the old code had: `rewrite_header` kept
/// the source's `CNAM` and appended the caller's, so a header came out with two.
#[test]
fn the_author_is_never_duplicated_and_survives_a_save() {
    let doc = create(Some("Original Author"), Some("Original description"));

    // Saving with no author named keeps the one the file has.
    let out = std::env::temp_dir().join("nvre-header-keep.esp");
    write_plugin(
        &doc,
        &MasterList::new(vec!["FalloutNV.esm".to_string()]),
        &nvre_esm::Schema::default(),
        &out,
    )
    .unwrap();
    let saved = Plugin::open(&out).unwrap();
    let cnam = saved
        .nodes
        .first()
        .and_then(|n| match n {
            nvre_esm::raw::Node::Record(r) => Some(r.subrecords().unwrap()),
            _ => None,
        })
        .expect("a TES4 record")
        .into_iter()
        .filter(|s| s.sig == *b"CNAM")
        .count();
    assert_eq!(
        cnam, 1,
        "exactly one CNAM, not the source's plus the caller's"
    );
    assert_eq!(saved.header.author.as_deref(), Some("Original Author"));
    assert_eq!(
        saved.header.description.as_deref(),
        Some("Original description")
    );
    std::fs::remove_file(&out).ok();
}
