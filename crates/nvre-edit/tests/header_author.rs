//! The TES4 header's author (`CNAM`) and description (`SNAM`).
//!
//! GECK's Create dialog has no author or description field of its own, but a
//! plugin's header carries them, and a plugin GECK saved with both is the
//! ground truth for how they are written:
//!
//! `testfiles/fonv_geck_author_desc.esm` (98 bytes) is
//! `HEDR CNAM "name" SNAM "desc" MAST FalloutNV.esm DATA`, with
//! `next_object_id` `0x00000DBA`. Re-saving it through NVRE must reproduce it
//! byte for byte, which is what the first test asserts.
//!
//! The rules the other tests pin down:
//!
//! - the order is `HEDR CNAM [SNAM] MAST/DATA [ONAM]` [measured: the file above,
//!   and 12 of the 13 plugins in the FNV Data folder];
//! - both are cstrings capped at 512 bytes including the terminator
//!   [documented: fopdoc's TES4 page];
//! - an empty description is no `SNAM`, not an empty one.

use nvre_edit::save::{write_plugin, MasterList};
use nvre_edit::Document;
use nvre_esm::Plugin;

/// `testfiles/fonv_geck_author_desc.esm`, byte for byte.
///
/// A master, with `CNAM` "name" and `SNAM` "desc" before the master list, and a
/// next-object-ID of `0x00000DBA` — the counter GECK had reached when it wrote
/// the file, which a save must not move backwards.
const GECK_AUTHOR_DESC_ESM: [u8; 98] = [
    0x54, 0x45, 0x53, 0x34, 0x4a, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x48, 0x45, 0x44, 0x52, 0x0c, 0x00, 0x1f, 0x85,
    0xab, 0x3f, 0x00, 0x00, 0x00, 0x00, 0xba, 0x0d, 0x00, 0x00, 0x43, 0x4e, 0x41, 0x4d, 0x05, 0x00,
    0x6e, 0x61, 0x6d, 0x65, 0x00, 0x53, 0x4e, 0x41, 0x4d, 0x05, 0x00, 0x64, 0x65, 0x73, 0x63, 0x00,
    0x4d, 0x41, 0x53, 0x54, 0x0e, 0x00, 0x46, 0x61, 0x6c, 0x6c, 0x6f, 0x75, 0x74, 0x4e, 0x56, 0x2e,
    0x65, 0x73, 0x6d, 0x00, 0x44, 0x41, 0x54, 0x41, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00,
];

/// Re-saving GECK's authored plugin reproduces it byte for byte.
///
/// This covers the author, the description, their order, and the next-object-ID
/// all at once: the counter is the part a save is most likely to get wrong,
/// because it is the only header field that is neither copied nor derived from a
/// subrecord the editor understands.
#[test]
fn a_saved_geck_authored_plugin_is_identical_to_gecks_file() {
    let doc = Document::from_plugin(
        Plugin::parse(&GECK_AUTHOR_DESC_ESM, "Authored.esm".to_string()).unwrap(),
    );
    assert_eq!(doc.plugin().header.author.as_deref(), Some("name"));
    assert_eq!(doc.plugin().header.description.as_deref(), Some("desc"));

    let out = std::env::temp_dir().join("nvre-header-geck-resave.esm");
    // The master list the file itself declares, which is what the session holds.
    write_plugin(
        &doc,
        &MasterList::new(doc.plugin().masters()),
        &nvre_esm::Schema::default(),
        &out,
    )
    .unwrap();

    assert_eq!(
        std::fs::read(&out).unwrap(),
        GECK_AUTHOR_DESC_ESM,
        "a save must reproduce GECK's header exactly, including its next-object-ID"
    );
    std::fs::remove_file(&out).ok();
}

/// The header's next-object-ID is a floor a save cannot go below.
///
/// GECK's authored plugin has no records of its own but advertises
/// `0x00000DBA`, because that is where its counter had reached. Deriving the
/// value from the records alone resets it to the constant for a fresh plugin,
/// which moves the numbering backwards and would hand the next record an ID the
/// plugin has already used.
#[test]
fn a_save_never_moves_the_next_object_id_backwards() {
    let doc = Document::from_plugin(
        Plugin::parse(&GECK_AUTHOR_DESC_ESM, "Authored.esm".to_string()).unwrap(),
    );
    let before = doc.plugin().header.next_object_id;
    assert_eq!(before, 0x0000_0DBA, "the file's own counter");

    let out = std::env::temp_dir().join("nvre-header-counter.esm");
    write_plugin(
        &doc,
        &MasterList::new(doc.plugin().masters()),
        &nvre_esm::Schema::default(),
        &out,
    )
    .unwrap();
    let saved = Plugin::open(&out).unwrap();
    assert_eq!(
        saved.header.next_object_id, before,
        "a file with no records must keep the counter it advertises, not reset it"
    );
    std::fs::remove_file(&out).ok();
}

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
