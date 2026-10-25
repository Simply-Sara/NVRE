//! The Size Info breakdown: how much of a file each record type occupies.
//!
//! The claim worth testing is the arithmetic. Every byte of a plugin is either a
//! record header, a record payload, a group header or the file header, so a
//! breakdown that adds up to the file size is attributing everything — and one
//! that does not is silently losing bytes, which is the one failure a size report
//! must never have.

use nvre_editor::file_details::{self, FileDetails, RowKind};
use nvre_editor::size_info;

/// A table of rows built by hand: `(kind, is_group, label, payload)`.
fn table(rows: &[(&str, RowKind, Option<&str>, u32)]) -> FileDetails {
    let mut d = FileDetails::default();
    for (kind, row_kind, label, payload) in rows {
        let k: [u8; 4] = kind.as_bytes().try_into().unwrap();
        d.row_kind.push(*row_kind);
        d.depth.push(1);
        d.kind.push(k);
        d.deleted.push(false);
        d.ignored.push(false);
        d.compressed.push(false);
        d.form_id.push(0);
        d.offset.push(0);
        d.length.push(*payload);
        d.vc.push(0);
        d.group_label
            .push(label.map(|l| l.as_bytes().try_into().unwrap()));
        d.editor_id.push(*label);
    }
    d
}

/// The breakdown accounts for every byte of the file it describes.
///
/// This is the property the window states out loud, so it is the one to pin: the
/// types' totals, plus the file header, plus the groups that belong to no type,
/// must equal the file size.
#[test]
fn every_byte_is_accounted_for() {
    // Two STAT records of 100 and 50 bytes, one WEAP of 30, their two group
    // headers, and a 54-byte file header.
    let d = table(&[
        ("TES4", RowKind::Header, None, 30),
        ("GRUP", RowKind::Group, Some("STAT"), 0),
        ("STAT", RowKind::Record, None, 100),
        ("STAT", RowKind::Record, None, 50),
        ("GRUP", RowKind::Group, Some("WEAP"), 0),
        ("WEAP", RowKind::Record, None, 30),
    ]);
    // 54 (header) + 24 (STAT group) + 124 + 74 (the two records) + 24 (WEAP
    // group) + 54 (the WEAP record) = 354
    let file_size = 354;
    let b = size_info::breakdown(&d, file_size);
    assert_eq!(
        b.total(),
        file_size,
        "the breakdown must add up to the file"
    );
    assert_eq!(b.header_bytes, 54, "the TES4 header and its payload");

    // Two types, largest first: STAT is 100+50 payloads plus two record headers
    // and one group header.
    assert_eq!(b.types.len(), 2);
    assert_eq!(b.types[0].kind, "STAT");
    assert_eq!(b.types[0].records, 2);
    assert_eq!(b.types[0].data, 150);
    assert_eq!(
        b.types[0].total,
        150 + 2 * 24 + 24,
        "payloads, two record headers, one group header"
    );
    assert_eq!(b.types[1].kind, "WEAP");
    assert_eq!(b.types[1].records, 1);
    assert_eq!(b.types[1].data, 30);
}

/// A group whose label is not a type is counted, but not as a type.
///
/// A cell's group is labelled with its grid coordinates and a worldspace's with
/// its FormID — four bytes of numbers. Treating those as type names produced
/// 11,252 "types" for `FalloutNV.esm` instead of 108, and left the total wrong by
/// nothing but made the report useless.
#[test]
fn a_group_with_no_type_label_is_not_a_type() {
    let d = table(&[
        ("TES4", RowKind::Header, None, 30),
        ("GRUP", RowKind::Group, Some("CELL"), 0),
        ("CELL", RowKind::Record, None, 40),
        // A nested group with no label: the cell's grid group.
        ("GRUP", RowKind::Group, None, 0),
        ("REFR", RowKind::Record, None, 60),
    ]);
    let file_size = 54 + 24 + (24 + 40) + 24 + (24 + 60);
    let b = size_info::breakdown(&d, file_size);

    assert_eq!(b.total(), file_size, "still every byte");
    assert_eq!(b.other_bytes, 24, "the unlabelled group's header");
    let kinds: Vec<&str> = b.types.iter().map(|t| t.kind.as_str()).collect();
    assert_eq!(
        kinds.len(),
        2,
        "CELL and REFR, and no label rows: {kinds:?}"
    );
    assert!(kinds.contains(&"CELL"));
    assert!(kinds.contains(&"REFR"));
}

/// A type that only appears as a group has no records and costs only its header.
#[test]
fn a_group_without_records_costs_its_header() {
    let d = table(&[
        ("TES4", RowKind::Header, None, 30),
        ("GRUP", RowKind::Group, Some("NAVM"), 0),
    ]);
    let b = size_info::breakdown(&d, 54 + 24);
    let navm = b.types.iter().find(|t| t.kind == "NAVM").unwrap();
    assert_eq!(navm.records, 0);
    assert_eq!(navm.data, 0);
    assert_eq!(navm.total, 24, "the group header and nothing else");
    assert_eq!(b.total(), 78);
}

/// The shares sum to one, since the types partition the attributed bytes.
#[test]
fn the_shares_are_a_fraction_of_the_file() {
    let d = table(&[
        ("TES4", RowKind::Header, None, 0),
        ("GRUP", RowKind::Group, Some("STAT"), 0),
        ("STAT", RowKind::Record, None, 1000),
        ("GRUP", RowKind::Group, Some("WEAP"), 0),
        ("WEAP", RowKind::Record, None, 1000),
    ]);
    let file_size = 24 + 24 + (24 + 1000) + 24 + (24 + 1000);
    let b = size_info::breakdown(&d, file_size);
    let sum: f64 = b.types.iter().map(|t| t.fraction(file_size)).sum();
    // Not exactly 1.0: the file header belongs to no type, so the types cover
    // everything but it.
    let header_share = b.header_bytes as f64 / file_size as f64;
    assert!(
        (sum + header_share - 1.0).abs() < 1e-9,
        "shares plus the header must be 1.0, got {sum} + {header_share}"
    );
}

/// A file of nothing but its header breaks down to nothing.
#[test]
fn a_header_only_file_has_no_types() {
    let d = table(&[("TES4", RowKind::Header, None, 30)]);
    let b = size_info::breakdown(&d, 54);
    assert!(b.types.is_empty());
    assert_eq!(b.header_bytes, 54);
    assert_eq!(b.total(), 54);
}

/// Sizes are printed at a scale a person can read.
#[test]
fn byte_counts_are_human_readable() {
    assert_eq!(size_info::human_bytes(0), "0 B");
    assert_eq!(size_info::human_bytes(512), "512 B");
    assert_eq!(size_info::human_bytes(1024), "1.0 KB");
    assert_eq!(size_info::human_bytes(1536), "1.5 KB");
    assert_eq!(size_info::human_bytes(1024 * 1024), "1.0 MB");
    assert_eq!(size_info::human_bytes(245_650_747), "234.3 MB");
}

/// The breakdown of a real plugin agrees with an independent walk of the file.
///
/// The unit tests above pin the arithmetic on hand-built tables; this pins it
/// against a real file, which is where a group label that is not a type and a
/// compressed record's `data_size` actually show up. Skipped when the game is not
/// installed, which is how every data-backed test here is run.
#[test]
fn a_real_plugin_breaks_down_exactly() {
    let Ok(data) = std::env::var("NVRE_DATA") else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    let path = std::path::Path::new(&data).join("FalloutNV.esm");
    if !path.exists() {
        eprintln!("skipped: no FalloutNV.esm in {data}");
        return;
    }
    let size = std::fs::metadata(&path).unwrap().len();
    let details = file_details::scan(&path).expect("scanned");
    let b = size_info::breakdown(&details, size);

    assert_eq!(
        b.total(),
        size,
        "every byte of a real plugin must be attributed"
    );
    // The two facts measured independently: LAND dominates a Fallout: New Vegas
    // master, and it is a small number of large records rather than many small
    // ones. A breakdown that got either wrong would still add up, so both are
    // worth asserting.
    let land = b.types.iter().find(|t| t.kind == "LAND").unwrap();
    assert_eq!(land.records, 29_363);
    assert_eq!(land.data, 164_847_006);
    assert!(
        land.fraction(size) > 0.6,
        "LAND is two thirds of the file, got {}",
        land.fraction(size)
    );
    let refr = b.types.iter().find(|t| t.kind == "REFR").unwrap();
    assert_eq!(refr.records, 307_710);
    assert!(
        refr.fraction(size) < 0.1,
        "REFR is many small records, got {}",
        refr.fraction(size)
    );
}
