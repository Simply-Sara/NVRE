//! A session created with **no masters**, which is what the launcher's
//! Create-New-Plugin tab now allows.
//!
//! GECK writes a plugin that depends on no master at all
//! (`testfiles/fonv_geck_empty_no_master.esp`: a TES4 header, 56 bytes), and the
//! editor has to be able to open one, place a record in it, and save it back
//! without a master appearing. Nothing here needs the game installed: the
//! schemas are compiled in, and a session with no masters has no plugin to read.

use nvre_editor::session::Session;

/// The schema directory argument: empty means the compiled-in schemas.
fn no_schema_dir() -> std::path::PathBuf {
    std::path::PathBuf::new()
}

/// A session with no masters opens, and reports an empty load order rather than
/// failing on the missing master.
#[test]
fn a_session_can_be_created_with_no_masters() {
    let dir = std::env::temp_dir().join("nvre-no-masters-session");
    std::fs::create_dir_all(&dir).unwrap();

    let session = Session::create(
        "NoMasters.esp",
        false,
        &dir,
        &no_schema_dir(),
        Vec::new(),
        None,
        None,
    )
    .expect("a plugin with no masters is a valid session");

    assert!(
        session.masters.is_empty(),
        "the session must depend on no master, got {:?}",
        session.masters
    );
    assert_eq!(
        session.plugins.len(),
        0,
        "there are no masters to load, so the load order holds no plugin"
    );
    assert!(
        session.doc.plugin().masters().is_empty(),
        "the document's own header must declare no masters"
    );
    assert_eq!(
        session.doc.plugin().self_index(),
        0,
        "with no masters the plugin's own index is 0"
    );
    // The header starts above the engine's reserved range, as GECK's file does.
    assert_eq!(
        session.doc.plugin().header.next_object_id,
        0x0000_0800,
        "a no-master plugin starts its numbering at 0x800"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// A new record's FormID in a no-master session is above the reserved range.
///
/// `next_local_form_id` is what a placed reference and a new cell are numbered
/// from, so this is the rule that keeps a standalone plugin's records out of the
/// engine's hardcoded FormID range.
#[test]
fn a_new_records_formid_clears_the_reserved_range() {
    let dir = std::env::temp_dir().join("nvre-no-masters-ids");
    std::fs::create_dir_all(&dir).unwrap();
    let session = Session::create(
        "NoMasters.esp",
        false,
        &dir,
        &no_schema_dir(),
        Vec::new(),
        None,
        None,
    )
    .unwrap();

    let fid = session.next_local_form_id();
    assert_eq!(
        fid, 0x0000_0800,
        "the first free FormID of a no-master plugin is 0x800, not 1"
    );
    assert!(
        fid & 0x00FF_FFFF >= 0x800,
        "a new record must not take an offset below 0x800 (engine-reserved)"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// The saved file keeps the empty master list.
///
/// The plausible failure is the save filling the master list in from the load
/// order, which would turn a deliberately standalone plugin into one that
/// depends on `FalloutNV.esm`.
#[test]
fn saving_a_no_master_session_adds_no_master() {
    let dir = std::env::temp_dir().join("nvre-no-masters-save");
    std::fs::create_dir_all(&dir).unwrap();
    let session = Session::create(
        "NoMasters.esp",
        false,
        &dir,
        &no_schema_dir(),
        Vec::new(),
        None,
        None,
    )
    .unwrap();

    let out = dir.join("NoMasters.esp");
    let report = session.save(&out).expect("a no-master session saves");
    assert_eq!(
        report.records, 0,
        "nothing was placed, so nothing is written"
    );

    let saved = nvre_esm::Plugin::open(&out).unwrap();
    assert!(
        saved.masters().is_empty(),
        "the saved file must still declare no masters, got {:?}",
        saved.masters()
    );
    assert_eq!(
        saved.header.next_object_id, 0x0000_0800,
        "and it keeps the numbering it was created with"
    );

    std::fs::remove_dir_all(&dir).ok();
}
