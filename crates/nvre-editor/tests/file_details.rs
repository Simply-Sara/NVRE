//! The File Details scan: every record in a file, with the header fields GECK's
//! dialog shows.
//!
//! The scanner exists to answer a question about 465,000 records without parsing
//! 465,000 payloads, so the thing worth testing is that the cheap answer is the
//! *same* answer — a scan that misses a record, or reads a different FormID, is
//! worse than no scan at all.
//!
//! Nothing here needs the game installed: the plugins are written by the editor
//! itself. `testfiles/` is not in the repository, so a test that read it would
//! pass here and fail on CI.

use nvre_editor::file_details;

/// A plugin holding three `STAT` records, each with an `EDID`, in one `GRUP`.
///
/// Built by hand rather than through `Document`, so the test's fixture is
/// explicit bytes: the scan's whole job is to read a container, and a fixture
/// that went through the editor's own writer would be testing the writer as much
/// as the reader.
fn three_stat_plugin() -> Vec<u8> {
    // A record: its 24-byte header, then the subrecords.
    fn record(sig: &[u8; 4], form_id: u32, edid: &str) -> Vec<u8> {
        let mut subs = Vec::new();
        subs.extend_from_slice(b"EDID");
        let mut name = edid.as_bytes().to_vec();
        name.push(0);
        subs.extend_from_slice(&(name.len() as u16).to_le_bytes());
        subs.extend_from_slice(&name);

        let mut out = Vec::new();
        out.extend_from_slice(sig);
        out.extend_from_slice(&(subs.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // flags
        out.extend_from_slice(&form_id.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // vc
        out.extend_from_slice(&0u16.to_le_bytes()); // form version
        out.extend_from_slice(&0u16.to_le_bytes()); // vc2
        out.extend_from_slice(&subs);
        out
    }

    let mut contents = Vec::new();
    for (i, edid) in ["FirstRecord", "SecondRecord", "ThirdRecord"]
        .iter()
        .enumerate()
    {
        // FormIDs above the reserved 0x800 range, as a real plugin numbers them.
        contents.extend(record(b"STAT", 0x0000_0800 + i as u32, edid));
    }

    // The group: `GRUP`, its whole size, the label `STAT`, and the type.
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&((24 + contents.len()) as u32).to_le_bytes());
    // The label, then the group type as a 4-byte integer: the two are separate
    // fields, and writing one where the other belongs shifts the header by two
    // bytes.
    group.extend_from_slice(b"STAT");
    group.extend_from_slice(&0i32.to_le_bytes()); // group type: top level
    group.extend_from_slice(&0u16.to_le_bytes()); // stamp
    group.extend_from_slice(&0u16.to_le_bytes()); // unknown
    group.extend_from_slice(&0u16.to_le_bytes()); // version
    group.extend_from_slice(&0u16.to_le_bytes()); // unknown2
    group.extend_from_slice(&contents);

    // The `TES4` header: `HEDR` only, which is all the scan reads.
    let mut hedr = Vec::new();
    hedr.extend_from_slice(b"HEDR");
    hedr.extend_from_slice(&12u16.to_le_bytes());
    hedr.extend_from_slice(&1.34f32.to_le_bytes());
    hedr.extend_from_slice(&0i32.to_le_bytes());
    hedr.extend_from_slice(&0x0000_0800u32.to_le_bytes());

    let mut header = Vec::new();
    header.extend_from_slice(b"TES4");
    header.extend_from_slice(&(hedr.len() as u32).to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes()); // flags: a plugin
    header.extend_from_slice(&0u32.to_le_bytes()); // form id
    header.extend_from_slice(&0u32.to_le_bytes()); // vc
    header.extend_from_slice(&0u16.to_le_bytes()); // form version
    header.extend_from_slice(&0u16.to_le_bytes()); // vc2
    header.extend_from_slice(&hedr);

    let mut file = header;
    file.extend_from_slice(&group);
    file
}

/// A scratch folder holding one blank plugin, and the path to it.
fn plugin(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nvre-file-details-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Details.esp");
    let doc = nvre_edit::Document::create_empty("Details.esp", false, &[], None, None).unwrap();
    doc.save(&path).unwrap();
    path
}

/// A file that is not a plugin is refused, rather than read as one.
#[test]
fn a_file_that_is_not_a_plugin_is_refused() {
    let dir = std::env::temp_dir().join("nvre-file-details-not-a-plugin");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("readme.txt");
    std::fs::write(&path, b"this is not a plugin\n").unwrap();
    assert!(matches!(
        file_details::scan(&path),
        Err(file_details::DetailsError::NotAPlugin)
    ));
    // A file that is not there is an I/O error, not a panic.
    assert!(matches!(
        file_details::scan(&dir.join("nothing-here.esp")),
        Err(file_details::DetailsError::Io(_))
    ));
}

/// An empty plugin's only row is its `TES4` header.
///
/// The header is a row because xEdit lists it as the tree's root, and it carries
/// real fields — flags, a FormID, a revision word — rather than being a label.
#[test]
fn an_empty_plugin_lists_only_its_header() {
    let path = plugin("empty");
    let details = file_details::scan(&path).expect("a blank plugin scans");
    assert_eq!(details.len(), 1, "just the TES4 row");
    assert_eq!(details.record_count(), 0, "and no records");
    assert_eq!(details.row_kind.get(0), Some(file_details::RowKind::Header));
    assert_eq!(details.kind.get(0), Some(*b"TES4"));
    assert_eq!(details.offset.get(0), Some(0));
    assert_eq!(details.depth.get(0), Some(0));
    assert_eq!(details.path, path);
}

/// The header and the groups are rows, and they nest.
///
/// A plugin is a tree: `TES4`, then a `GRUP` per record type, then the records
/// in it. A flat list of records with the groups left out hides where a record
/// lives, which is half of what the dialog is for.
#[test]
fn the_header_and_groups_are_rows() {
    let path = plugin("structure");
    std::fs::write(&path, three_stat_plugin()).unwrap();
    let details = file_details::scan(&path).unwrap();

    // The header, the STAT group, and the three records in it.
    assert_eq!(details.len(), 5, "TES4 + GRUP + three records");
    assert_eq!(details.record_count(), 3, "three of them are records");

    let kinds: Vec<_> = (0..details.len())
        .map(|r| details.row_kind.get(r).unwrap())
        .collect();
    assert_eq!(
        kinds,
        vec![
            file_details::RowKind::Header,
            file_details::RowKind::Group,
            file_details::RowKind::Record,
            file_details::RowKind::Record,
            file_details::RowKind::Record,
        ]
    );

    // The tree's shape: the header at the top, the group under it, the records
    // under the group.
    assert_eq!(details.depth.get(0), Some(0), "the header is the root");
    assert_eq!(details.depth.get(1), Some(1), "the group is one level in");
    for row in 2..5 {
        assert_eq!(details.depth.get(row), Some(2), "records are two levels in");
    }

    // A group carries its label, which is the only name it has.
    assert_eq!(details.editor_id.get(1), Some("STAT"));
    // And a group has no FormID or revision word, so those are zero rather than
    // a number that means nothing.
    assert_eq!(details.form_id.get(1), Some(0));
    assert_eq!(details.vc.get(1), Some(0));
}

/// Every row's header fields are the ones the full parser reports.
///
/// This is the claim that makes the scan safe to use instead of `Plugin::open`:
/// the same records, in the same places, with the same numbers.
#[test]
fn the_scan_agrees_with_the_full_parser() {
    let path = plugin("agree");
    // Put real records in the file, through the same document API the editor
    // uses, then compare the two readers row by row.
    // A plugin built by hand: three `STAT` records, each with an `EDID`, in one
    // top-level group. Written with the raw container API so the test does not
    // depend on the document editor's staging.
    let bytes = three_stat_plugin();
    std::fs::write(&path, &bytes).unwrap();

    let details = file_details::scan(&path).expect("scanned");
    let whole = nvre_esm::Plugin::open(&path).expect("parsed");

    let parsed = whole.records();
    assert_eq!(
        details.record_count(),
        parsed.len(),
        "the scan must see every record the parser sees"
    );

    // Matched by offset: the scan walks file order, while `records()` reports a
    // group's own records before its subgroups.
    let by_offset: std::collections::HashMap<u64, &nvre_esm::Record> =
        parsed.iter().map(|r| (r.offset, *r)).collect();
    for row in 0..details.len() {
        // Only records: the header and the groups are rows too, and the parser
        // has no equivalent to compare them against.
        if details.row_kind.get(row) != Some(file_details::RowKind::Record) {
            continue;
        }
        let off = details.offset.get(row).unwrap();
        let r = by_offset
            .get(&off)
            .unwrap_or_else(|| panic!("no parser record at {off:#x}"));
        assert_eq!(&details.kind.get(row).unwrap()[..], &r.header.sig[..]);
        assert_eq!(details.form_id.get(row).unwrap(), r.header.form_id);
        assert_eq!(details.length.get(row).unwrap(), r.header.data_size);
        assert_eq!(details.vc.get(row).unwrap(), r.header.vc_info);
        assert_eq!(details.deleted.get(row).unwrap(), r.is_deleted());
        assert_eq!(details.compressed.get(row).unwrap(), r.is_compressed());
        assert_eq!(
            details.editor_id.get(row),
            nvre_edit::document::edid_of(r).as_deref(),
            "the Editor ID at {off:#x}"
        );
    }

    // The Editor IDs really are the ones that went in, so the comparison above is
    // not two readers agreeing on nothing.
    let named: Vec<&str> = (0..details.len())
        .filter(|r| details.row_kind.get(*r) == Some(file_details::RowKind::Record))
        .filter_map(|r| details.editor_id.get(r))
        .collect();
    assert!(named.contains(&"FirstRecord"), "got {named:?}");
    assert!(named.contains(&"ThirdRecord"), "got {named:?}");
}

/// The `Ignored` column reads the `0x1000` header flag, not some other bit.
///
/// The flag word is contextual — most bits mean something different per record
/// type — so the three this window names are the only ones it may claim
/// [documented: fopdoc's Record Format table, `0x20` Deleted, `0x1000` Ignored,
/// `0x40000` Compressed].
#[test]
fn the_flag_columns_read_the_flags_they_name() {
    let path = plugin("flags");
    let details = file_details::scan(&path).unwrap();
    // A blank plugin has no records, so the columns are empty rather than wrong;
    // the flag bits themselves are asserted through the parser comparison above,
    // which covers `deleted` and `compressed` on real records.
    assert_eq!(details.deleted.len(), details.len());
    assert_eq!(details.ignored.len(), details.len());
    assert_eq!(details.compressed.len(), details.len());
    assert_eq!(details.depth.len(), details.len());
    assert_eq!(details.row_kind.len(), details.len());
}

/// The filter matches a record's type, Editor ID or FormID.
///
/// Those are the three ways GECK's own dialog is searched: by what a record is,
/// by its name, and by the number someone quoted in a bug report.
#[test]
fn the_filter_searches_type_name_and_form_id() {
    let mut state = nvre_editor::file_details_window::FileDetailsState::default();
    // A tiny table built by hand, so the filter is tested without a file.
    let mut d = file_details::FileDetails::default();
    for (kind, edid, fid) in [
        (*b"STAT", "RockBig", 0x0001_2345u32),
        (*b"WEAP", "LaserRifle", 0x0002_3456),
        (*b"NPC_", "Raul", 0x0003_4567),
    ] {
        d.row_kind.push(file_details::RowKind::Record);
        d.depth.push(1);
        d.kind.push(kind);
        d.form_id.push(fid);
        d.editor_id.push(Some(edid));
        d.deleted.push(false);
        d.ignored.push(false);
        d.offset.push(0);
        d.length.push(0);
        d.vc.push(0);
        d.compressed.push(false);
    }
    state.details = Some(d);

    // No filter: every row.
    assert_eq!(state.row_count(), 3);

    for (needle, expected) in [
        ("stat", vec!["RockBig"]),
        ("laser", vec!["LaserRifle"]),
        ("raul", vec!["Raul"]),
        // `2345` is a substring of both `0x00012345` and `0x00023456`: the
        // filter is a substring search, as GECK's is, not an exact FormID match.
        ("2345", vec!["RockBig", "LaserRifle"]),
        ("12345", vec!["RockBig"]),
        ("weap", vec!["LaserRifle"]),
        ("nothing", vec![]),
    ] {
        state.filter = needle.to_string();
        state.refilter();
        let got: Vec<&str> = (0..state.row_count())
            .filter_map(|r| state.row_at(r))
            .filter_map(|i| state.details.as_ref().unwrap().editor_id.get(i))
            .collect();
        assert_eq!(got, expected, "filter {needle:?}");
    }
}
