//! The Objects window lists what the **mod** carries, not what the masters do.
//!
//! The list is built by walking the load order, which is a snapshot of the
//! plugins on disk taken when the session opened. An edit queued this session
//! and a duplicate staged this session are in neither, so without an overlay
//! the window keeps showing the record as it was — an EDID typed into the
//! object editor and applied does not appear until the plugin is saved and
//! reopened.
//!
//! These tests drive the same calls the panels make and check that both show
//! up on the next read, without a save and without changing the filter.

use nvre_editor::session::{ObjectRow, Session};

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

fn schema_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")
}

/// Open the master as a session, the way the editor does.
fn session() -> Option<Session> {
    let data = data_dir()?;
    let plugin = std::path::Path::new(&data).join("FalloutNV.esm");
    Session::open(&plugin, &schema_dir(), vec!["FalloutNV.esm".to_string()]).ok()
}

/// The first object of a category, for a test to act on.
fn first_row(groups: &[nvre_editor::session::ObjectGroup], category: &str) -> ObjectRow {
    groups
        .iter()
        .find(|g| g.category == category)
        .and_then(|g| g.objects.first())
        .cloned()
        .unwrap_or_else(|| panic!("a {category} in the object window"))
}

/// A duplicate is listed the moment it is created, in the same category as
/// the record it was copied from, under the Editor ID it was given.
///
/// This is the whole point of the overlay: the copy is in no plugin on disk,
/// so the load-order walk that builds the list never sees it. Only the
/// session's staged records know it exists.
#[test]
fn a_duplicate_is_listed_immediately() {
    let Some(mut s) = session() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let stat = first_row(&s.object_groups("").to_vec(), "Static");
    let before: usize = s
        .object_groups("")
        .iter()
        .find(|g| g.category == "Static")
        .map(|g| g.objects.len())
        .expect("the Static category");

    let new = s
        .duplicate_object(stat.fid, *b"STAT", "NvreDuplicateNow")
        .expect("the duplicate is staged");

    // No save, and the filter has not changed: the same call the panel makes
    // every frame has to return the new row.
    let groups = s.object_groups("").to_vec();
    let statics = groups
        .iter()
        .find(|g| g.category == "Static")
        .expect("the Static category is still there");
    assert_eq!(
        statics.objects.len(),
        before + 1,
        "the duplicate is one more row in the same category"
    );
    let row = statics
        .objects
        .iter()
        .find(|r| r.fid == new)
        .unwrap_or_else(|| panic!("the duplicate {new:08X} is in the Static category"));
    assert_eq!(
        row.edid, "NvreDuplicateNow",
        "it carries the Editor ID it was given"
    );
    // It is a copy, so it keeps the original's name: the duplicate prompt
    // names the Editor ID, not the display name.
    assert_eq!(row.name, stat.name, "the name is the original's");
}

/// A duplicate is filtered like any other object.
///
/// A row that ignores the filter would be listed under every search, which
/// makes a filtered window lie about how many objects match.
#[test]
fn a_duplicate_is_filtered_like_any_other_object() {
    let Some(mut s) = session() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let stat = first_row(&s.object_groups("").to_vec(), "Static");
    s.duplicate_object(stat.fid, *b"STAT", "NvreFilteredDuplicate")
        .expect("the duplicate is staged");

    let matching = s.object_groups("NvreFilteredDuplicate").to_vec();
    let shown: Vec<&ObjectRow> = matching
        .iter()
        .flat_map(|g| g.objects.iter())
        .filter(|r| r.fid != stat.fid)
        .collect();
    assert_eq!(shown.len(), 1, "the duplicate matches its own Editor ID");
    assert_eq!(shown[0].edid, "NvreFilteredDuplicate");

    // And a filter it does not match leaves it out, rather than showing it
    // regardless.
    let other = s.object_groups("dumpster").to_vec();
    assert!(
        other
            .iter()
            .flat_map(|g| g.objects.iter())
            .all(|r| r.edid != "NvreFilteredDuplicate"),
        "a duplicate that does not match the filter is not listed"
    );
}

/// An EDID edit is visible in the list without a save.
///
/// The list used to read the load order only, so a record renamed in the
/// object editor kept its old Editor ID in the window — and, because the
/// editor's own grid reloaded only on a change of target, in the editor too.
#[test]
fn an_edid_edit_is_listed_immediately() {
    let Some(mut s) = session() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let stat = first_row(&s.object_groups("").to_vec(), "Static");
    let original = stat.edid.clone();

    // What the object editor's Apply does, through the session's own field
    // writer: the EDID is a zstring, so the bytes are the encoded text.
    let bytes = nvre_edit::document::encode_zstring("NvreRenamedStatic");
    s.selected = Some(stat.fid);
    s.edit_selected(b"EDID", bytes);

    let groups = s.object_groups("").to_vec();
    let row = groups
        .iter()
        .flat_map(|g| g.objects.iter())
        .find(|r| r.fid == stat.fid)
        .unwrap_or_else(|| panic!("{:08X} is still listed", stat.fid));
    assert_eq!(
        row.edid, "NvreRenamedStatic",
        "the list shows the new Editor ID, not {original}"
    );

    // The edit is queued on the document, so the saved plugin will carry it
    // too — the list is not showing something the file will not have.
    let edits = s.doc.edits_for(stat.fid).expect("the edit is queued");
    assert!(
        edits.iter().any(|e| matches!(
            e,
            nvre_edit::Edit::SetSub { sig, .. } if sig == b"EDID"
        )),
        "the EDID edit is queued for the save"
    );
}

/// A record queued for deletion leaves the list.
///
/// The game will not have it, so the window should not either: listing a
/// record that is about to be written with the Deleted flag set is a row you
/// can open and edit but never see in-game.
#[test]
fn a_deleted_record_leaves_the_list() {
    let Some(mut s) = session() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let stat = first_row(&s.object_groups("").to_vec(), "Static");
    assert!(
        s.object_groups("")
            .iter()
            .flat_map(|g| g.objects.iter())
            .any(|r| r.fid == stat.fid),
        "the record is listed before it is deleted"
    );

    s.select(stat.fid);
    s.delete_selected();

    let groups = s.object_groups("").to_vec();
    assert!(
        groups
            .iter()
            .flat_map(|g| g.objects.iter())
            .all(|r| r.fid != stat.fid),
        "a deleted record is not listed"
    );
}

/// The overlay covers only the FormIDs the document touches, so an untouched
/// A duplicate opens in the editor with the data it was copied with.
///
/// The record is in no plugin on disk, so an editor that reads the plugin and
/// the load order finds nothing and shows an empty grid. Reading through the
/// resolver is what fills it.
#[test]
fn a_duplicate_opens_in_the_editor_with_its_data() {
    let Some(mut s) = session() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let stat = first_row(&s.object_groups("").to_vec(), "Static");
    let original_subs = s
        .resolved_record(stat.fid)
        .expect("the original resolves")
        .subrecords()
        .expect("the original's subrecords parse");

    let new = s
        .duplicate_object(stat.fid, *b"STAT", "NvreOpensWithData")
        .expect("the duplicate is staged");

    // What the object editor reads when it opens the record.
    let rec = s
        .resolved_record(new)
        .unwrap_or_else(|| panic!("the duplicate {new:08X} resolves for the editor"));
    let subs = rec.subrecords().expect("the duplicate's subrecords parse");
    assert_eq!(
        subs.len(),
        original_subs.len(),
        "the duplicate opens with every subrecord it was copied with"
    );
    assert_eq!(rec.header.sig, *b"STAT", "it keeps the record type");
    assert_eq!(
        nvre_util::cp1252::decode_z(
            subs.iter()
                .find(|x| &x.sig == b"EDID")
                .map(|x| x.data.as_slice())
                .unwrap_or_default()
        ),
        "NvreOpensWithData",
        "it opens under the Editor ID it was given"
    );
}

/// An edit queued on a fresh duplicate is visible and survives the save.
///
/// Two halves, both of which were broken: the resolver returned the staged
/// record without applying the edits queued on it, so the editor showed the
/// old value; and the save wrote the staged record without applying them
/// either, so the edit was queued and then silently dropped.
#[test]
fn an_edit_to_a_duplicate_is_shown_and_saved() {
    let Some(mut s) = session() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let stat = first_row(&s.object_groups("").to_vec(), "Static");
    let new = s
        .duplicate_object(stat.fid, *b"STAT", "NvreEditAfterDuplicate")
        .expect("the duplicate is staged");

    // What the object editor's Apply Changes queues.
    s.selected = Some(new);
    s.edit_selected(
        b"MODL",
        nvre_edit::document::encode_zstring("clutter\\nvetest\\edited.nif"),
    );

    // Shown: the resolver applies the edit to the staged record.
    let shown = s.resolved_record(new).expect("still resolves");
    let modl = shown
        .subrecords()
        .expect("subrecords parse")
        .into_iter()
        .find(|x| &x.sig == b"MODL")
        .map(|x| nvre_util::cp1252::decode_z(&x.data))
        .unwrap_or_default();
    assert_eq!(
        modl, "clutter\\nvetest\\edited.nif",
        "the resolver shows the edit queued after the duplicate was created"
    );

    // Saved: the staged record is written with the edit applied.
    let out = std::env::temp_dir().join("NvreDuplicateEdit.esp");
    s.save(&out).expect("the save works");
    let saved = nvre_esm::Plugin::open(&out).expect("the saved file parses");
    let mut written: Vec<u8> = Vec::new();
    nvre_esm::raw::visit_nodes(&saved.nodes, &mut |r, _| {
        if r.header.form_id >> 24 == saved.masters().len() as u32 {
            if let Ok(subs) = r.subrecords() {
                if let Some(edid) = subs.iter().find(|x| &x.sig == b"EDID") {
                    if nvre_util::cp1252::decode_z(&edid.data) == "NvreEditAfterDuplicate" {
                        written = subs
                            .iter()
                            .find(|x| &x.sig == b"MODL")
                            .map(|x| x.data.clone())
                            .unwrap_or_default();
                    }
                }
            }
        }
    });
    assert_eq!(
        nvre_util::cp1252::decode_z(&written),
        "clutter\\nvetest\\edited.nif",
        "the saved copy carries the edit, not the original's model"
    );
    std::fs::remove_file(&out).ok();
}

/// An untouched record's row is the load order's own answer.
///
/// The performance property that makes the overlay affordable: 44,550 objects
/// are listed, and a handful are touched, so the cost is a hash lookup per
/// row rather than a subrecord parse per row.
#[test]
fn an_untouched_record_is_listed_from_the_load_order() {
    let Some(mut s) = session() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let stat = first_row(&s.object_groups("").to_vec(), "Static");
    let before = s.object_groups("").to_vec();
    let before_row = before
        .iter()
        .flat_map(|g| g.objects.iter())
        .find(|r| r.fid == stat.fid)
        .cloned()
        .expect("the Static is listed");

    // Touch a different record entirely.
    let other = first_row(&before, "Activator");
    s.selected = Some(other.fid);
    s.edit_selected(b"EDID", nvre_edit::document::encode_zstring("NvreOther"));

    let after = s.object_groups("").to_vec();
    let row = after
        .iter()
        .flat_map(|g| g.objects.iter())
        .find(|r| r.fid == stat.fid)
        .cloned()
        .expect("the Static is still listed");
    assert_eq!(
        (row.edid, row.name),
        (before_row.edid.clone(), before_row.name.clone()),
        "an untouched record's row is unchanged"
    );
}
