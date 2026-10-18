//! What the references table's columns hold.
//!
//! The table shows Editor ID, Form ID, Type, Owner and Lock. The last two are
//! only useful if the game's records carry them, and most of a cell's
//! references carry neither: ownership is set on some things and locks on doors
//! and containers. So the columns are checked against the data that is actually
//! there — a cell that has an owned reference and a locked one, and one that has
//! neither.

use nvre_editor::session::Session;

fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

fn open_master() -> Option<Session> {
    let data = data_dir()?;
    Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}

/// Open the cell with this Editor ID.
fn open(s: &mut Session, edid: &str) -> u32 {
    let cell = s
        .index
        .cells
        .values()
        .find(|c| c.edid.as_deref() == Some(edid))
        .map(|c| c.form_id)
        .unwrap_or_else(|| panic!("{edid}"));
    s.open_cell(cell);
    cell
}

/// A base FormID's record type, the way the table resolves it.
fn base_type(s: &mut Session, r: &nvre_editor::session::RefRow) -> Option<String> {
    let base = u32::from_str_radix(r.base.as_deref()?.strip_prefix("0x")?, 16).ok()?;
    s.ref_details().base_type(base).map(str::to_string)
}

/// What the Editor ID column shows for a row: its own EDID when it has one, else
/// its base object's, else its signature and FormID.
fn editor_id_cell(s: &mut Session, r: &nvre_editor::session::RefRow) -> String {
    let base = r
        .base
        .as_deref()
        .and_then(|b| u32::from_str_radix(b.strip_prefix("0x")?, 16).ok());
    r.edid
        .clone()
        .or_else(|| {
            base.and_then(|b| s.ref_details().base_edid(b))
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("{} {:08X}", r.sig, r.form_id))
}

#[test]
fn the_editor_id_column_names_a_placed_object_by_what_it_places() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    open(&mut s, "Goodsprings");
    let refs = s.cell_refs().to_vec();
    // A reference's own EDID is the exception: one of Goodsprings' 159 carries
    // one [observed: fnvtool esm survey finds 5,011 of 307,710 REFRs have one],
    // so naming a row by its own alone leaves 98% of the column blank — which
    // is what the column used to fall back on.
    let with_own = refs.iter().filter(|r| r.edid.is_some()).count();
    assert!(
        with_own * 50 < refs.len(),
        "a cell's references mostly carry no EDID of their own; {with_own} of {} did",
        refs.len()
    );
    // Every row is named anyway, by the object it places.
    let unnamed = refs
        .iter()
        .filter(|r| editor_id_cell(&mut s, r).starts_with("REFR "))
        .count();
    assert_eq!(
        unnamed,
        0,
        "every row is named: {unnamed} of {} fell back to the signature and FormID",
        refs.len()
    );
    // And the name is the base object's Editor ID, not the FormID: a column of
    // "0001EE8F" would be as unreadable as the blank it replaced.
    let first = &refs[0];
    let edid = editor_id_cell(&mut s, first);
    assert_eq!(edid, "WastelandRoadStraightLongB");
    assert!(
        !edid.starts_with(|c: char| c.is_ascii_digit()),
        "the name is an Editor ID, not a FormID: {edid}"
    );
}

#[test]
fn the_type_column_names_the_base_object_the_way_geck_does() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    open(&mut s, "Goodsprings");
    let refs = s.cell_refs().to_vec();
    // Nearly every reference in a cell has a base object, and each one's type
    // is what the column says.
    let typed = refs
        .iter()
        .filter(|r| base_type(&mut s, r).is_some())
        .count();
    assert!(
        typed * 20 > refs.len() * 19,
        "a reference places a base object; {typed} of {} had one",
        refs.len()
    );
    // The type is the schema's own name for the record, not its signature:
    // GECK's render window says "Static" where the file says STAT.
    for r in refs.iter().take(50) {
        if let Some(t) = base_type(&mut s, r) {
            assert!(
                !(t.len() == 4 && t.bytes().all(|b| b.is_ascii_uppercase())),
                "a type is GECK's name, not a signature: {t}"
            );
            assert!(!t.is_empty(), "a type is never blank");
        }
    }
    // And it reads the way GECK's does for the types a cell actually holds.
    let seen: Vec<String> = refs.iter().filter_map(|r| base_type(&mut s, r)).collect();
    for want in ["Static", "Static Collection", "Moveable Static"] {
        assert!(
            seen.iter().any(|t| t == want),
            "Goodsprings holds {want}; the column shows {seen:?}"
        );
    }
    // The type is also the dropdown's identity, so it has to be unique per
    // record type — a collision would make the filter hide rows it named. That
    // is a property of the schema, so it is checked across every record type
    // rather than only the ones this cell happens to hold.
    let mut names: Vec<&str> = s.schema.records.values().map(|d| d.name.as_str()).collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        total,
        "two record types share a name, so a type filter would be ambiguous"
    );
}

#[test]
fn the_lock_column_carries_the_xloc_level_of_locked_references() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // VendorChestsCell: nine references, six of them locked, so a cell that
    // carries XLOC at all can be checked without scrolling.
    open(&mut s, "VendorChestsCell");
    let refs = s.cell_refs().to_vec();
    let locked: Vec<&nvre_editor::session::RefRow> =
        refs.iter().filter(|r| r.lock.is_some()).collect();
    assert!(
        !locked.is_empty(),
        "an interior with locked doors carries XLOC on them"
    );
    for r in &locked {
        let lock = r.lock.expect("a lock");
        // A level the game reports: 0 Very Easy … 100 Very Hard, or 255 for a
        // lock only the key opens.
        assert!(
            lock.level == 255 || lock.level <= 100,
            "{}: lock level {} is not one the game reports",
            r.form_id,
            lock.level
        );
        // The level is what the column prints, so it reads back.
        assert_eq!(
            lock.label(),
            if lock.level == 255 {
                "key".to_string()
            } else {
                lock.level.to_string()
            }
        );
    }
    // And a reference with no XLOC has no lock to show, which is most of a cell.
    let unlocked = refs.len() - locked.len();
    assert!(unlocked > 0, "a cell holds more than its locked references");
}

#[test]
fn the_owner_column_resolves_an_owned_reference() {
    let Some(mut s) = open_master() else {
        eprintln!("skipped: set NVRE_DATA to run against the game's masters");
        return;
    };
    // VendorChestsCell: the same nine references, eight of them owned, so the
    // column is checked against a cell that actually sets ownership.
    open(&mut s, "VendorChestsCell");
    let refs = s.cell_refs().to_vec();
    let owned: Vec<&nvre_editor::session::RefRow> =
        refs.iter().filter(|r| r.owner.is_some()).collect();
    assert!(!owned.is_empty(), "an interior has owned references in it");
    let details = s.ref_details().clone();
    for r in &owned {
        let owner = r.owner.expect("an owner");
        // Every owner resolves to a name: the record's Editor ID, or the
        // FormID when it has none. Never blank.
        let name = details.owner(owner).expect("a resolved owner");
        assert!(!name.is_empty(), "{owner:08X} resolved to nothing");
    }
    // Ownership is not universal: the same cell holds references nobody owns,
    // and those rows have no owner FormID to resolve.
    assert!(
        owned.len() < refs.len(),
        "a cell holds both owned and unowned references"
    );
}
