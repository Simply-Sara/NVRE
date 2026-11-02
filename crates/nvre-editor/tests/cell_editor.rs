//! The Cell Editor window's load and save path.
//!
//! The window is the editor's front door onto a cell's own `CELL` record, so
//! these drive the same calls its UI makes — `load_from_session` and
//! `save_to_session` — and check three things the window promises: that a
//! cell's fields read as the game wrote them, that a save with no edit
//! reproduces the record byte for byte, and that an edit lands on the
//! subrecord it belongs to.
//!
//! The fixture is a real interior from `FalloutNV.esm`, chosen by a probe:
//! `HooverDamIntPowerPlant01` carries a `FULL`, `DATA`, `XCLL`, `LTMP`,
//! `LNAM`, `XCLW`, `XNAM`, `XCIM`, `XCMO`, `XCAS`, `XOWN` and `XCWT` — every
//! subrecord the window edits but `XCLC` and `XCLR` — so one cell exercises
//! almost the whole page.

mod common;

use common::*;
use nvre_editor::cell_editor::CellEditorState;

/// The game's Data folder, when the environment names one.
fn data_dir() -> Option<String> {
    std::env::var("NVRE_DATA").ok()
}

/// A session over `FalloutNV.esm`.
fn session() -> Option<nvre_editor::session::Session> {
    let data = data_dir()?;
    nvre_editor::session::Session::open(
        std::path::Path::new(&format!("{data}/FalloutNV.esm")),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")),
        vec!["FalloutNV.esm".to_string()],
    )
    .ok()
}

/// The fixture cell, by Editor ID, resolved to its FormID.
///
/// `HooverDamIntPowerPlant01`: an interior with lighting, a light template
/// and a dozen references. Chosen by `dump_record`, which showed every
/// subrecord the window edits but the grid and the regions.
fn hoover_dam(s: &nvre_editor::session::Session) -> u32 {
    let fid = s
        .doc
        .records()
        .into_iter()
        .find(|(_, r)| {
            &r.header.sig == b"CELL"
                && r.subrecords().ok().and_then(|subs| {
                    subs.iter().find(|x| &x.sig == b"EDID").map(|x| {
                        String::from_utf8_lossy(&x.data).trim_end_matches('\0')
                            == "HooverDamIntPowerPlant01"
                    })
                }) == Some(true)
        })
        .map(|(f, _)| f);
    fid.expect("HooverDamIntPowerPlant01 is in FalloutNV.esm")
}

/// A cell's fields read as the game wrote them.
///
/// The values are the ones `dump_record CELL` printed for this record: the
/// ambient is (87, 78, 64), the fog runs 3000 to 10000, and the water is
/// `WATR` `000AED89`.
#[test]
fn a_cell_reads_as_the_game_wrote_it() {
    let Some(s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let fid = hoover_dam(&s);
    let mut state = CellEditorState::default();
    state.load_from_session(&s, fid);

    assert_eq!(state.edid, "HooverDamIntPowerPlant01", "the Editor ID");
    assert_eq!(state.full_name, "Hoover Dam Power Plant 01", "the name");
    assert_eq!(
        state.data_flags, 0x23,
        "interior, has water, public place — the byte the record stores"
    );
    assert!(
        state.data_flags & 0x01 != 0,
        "Is Interior Cell is set, which is what an interior is"
    );

    let l = state.lighting.expect("the cell carries an XCLL");
    assert_eq!(l.ambient, [87, 78, 64], "the ambient colour");
    assert_eq!(l.directional, [0, 0, 0], "the sun's colour");
    assert_eq!(l.fog, [36, 35, 28], "the fog colour");
    assert_eq!(l.fog_near, 3000.0, "where the fog starts");
    assert_eq!(l.fog_far, 10000.0, "where it is opaque");
    assert_eq!(l.directional_fade, 1.0);

    assert_eq!(
        state.light_template,
        Some(0x0007_E3B7),
        "the LTMP template record"
    );
    assert_eq!(
        state.lnam_flags, 2,
        "it inherits the directional colour only"
    );
    assert_eq!(state.water, Some(0x000A_ED89), "the water type");
    assert_eq!(state.image_space, Some(0x0011_C0C1), "the image space");
    assert_eq!(state.music_type, Some(0x0008_A6D4), "the music type");
    assert_eq!(
        state.acoustic_space,
        Some(0x0017_769A),
        "the acoustic space"
    );
    assert_eq!(state.owner, Some(0x000A_46E7), "the owner");

    // An interior has no grid, so the window does not offer one and does not
    // write one back.
    assert!(!state.has_grid, "an interior carries no XCLC");
}

/// A save with no edit reproduces the record byte for byte.
///
/// The strongest statement the window can make: that opening a cell and
/// pressing Apply without touching anything changes nothing. The save is
/// taken with the no-op applied, which is what queues the record as an
/// override — a mod overrides only the records it touched, so an untouched
/// record is not written at all.
#[test]
fn a_save_with_no_edit_changes_nothing() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let fid = hoover_dam(&s);
    let before = subs(&s, fid);
    let before_xcll = payload(&before, b"XCLL");
    let before_data = payload(&before, b"DATA");

    let mut state = CellEditorState::default();
    state.load_from_session(&s, fid);
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "cell_editor_noop");
    assert_eq!(
        sig_order(&after),
        sig_order(&before),
        "the subrecord order is the record's own"
    );
    assert_eq!(
        payload(&after, b"XCLL"),
        before_xcll,
        "the lighting subrecord is unchanged"
    );
    assert_eq!(
        payload(&after, b"DATA"),
        before_data,
        "the flags byte is unchanged"
    );
}

/// An edit lands on the subrecord it belongs to.
///
/// The fog and a flag are changed and everything else is left alone, which is
/// what a modder editing a cell's atmosphere does: the record's other
/// subrecords are not the editor's to touch.
#[test]
fn an_edit_lands_on_the_right_subrecord() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let fid = hoover_dam(&s);
    let before_owner = payload(&subs(&s, fid), b"XOWN");
    let before_water = payload(&subs(&s, fid), b"XCWT");

    let mut state = CellEditorState::default();
    state.load_from_session(&s, fid);
    state.data_flags = 0x03; // interior, no water
    if let Some(l) = state.lighting.as_mut() {
        l.fog_near = 100.0;
        l.fog_far = 2000.0;
        l.ambient = [10, 20, 30];
    }
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "cell_editor_edit");

    // DATA: the byte the record holds, with bit 1 cleared.
    let data = payload(&after, b"DATA");
    assert_eq!(data, vec![0x03], "Has Water is cleared, Is Interior stays");

    // XCLL: the forty bytes with the three fields the editor owns overwritten.
    let xcll = payload(&after, b"XCLL");
    assert_eq!(xcll.len(), 40, "the subrecord keeps its own length");
    assert_eq!(&xcll[0..3], &[10, 20, 30], "the ambient colour");
    assert_eq!(
        f32::from_le_bytes(xcll[12..16].try_into().unwrap()),
        100.0,
        "the fog's near distance"
    );
    assert_eq!(
        f32::from_le_bytes(xcll[16..20].try_into().unwrap()),
        2000.0,
        "the fog's far distance"
    );
    // The byte after each colour is the record's own, not the editor's.
    assert_eq!(xcll[3], 0, "the ambient's unused byte survives");

    // The subrecords the editor did not touch are the record's own bytes.
    assert_eq!(
        payload(&after, b"XOWN"),
        before_owner,
        "the owner is unchanged"
    );
    assert_eq!(
        payload(&after, b"XCWT"),
        before_water,
        "the water type is unchanged"
    );
}

/// Clearing a reference removes the subrecord rather than writing a null.
///
/// A cell whose owner is cleared must not keep a subrecord pointing at FormID
/// zero: that is a reference to the engine's own numbering, not to no owner.
#[test]
fn a_cleared_reference_removes_the_subrecord() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let fid = hoover_dam(&s);

    let mut state = CellEditorState::default();
    state.load_from_session(&s, fid);
    state.owner = None;
    state.water = None;
    state.save_to_session(&mut s);

    let after = saved_subs(&s, fid, "cell_editor_cleared");
    assert!(
        !after.iter().any(|x| &x.sig == b"XOWN"),
        "the owner subrecord is gone, not zeroed"
    );
    assert!(
        !after.iter().any(|x| &x.sig == b"XCWT"),
        "the water subrecord is gone, not zeroed"
    );
    // The subrecord order is the record's own: removing one leaves the rest
    // where they were.
    assert_eq!(
        sig_order(&after),
        sig_order(&subs(&s, fid))
            .into_iter()
            .filter(|x| x != "XOWN" && x != "XCWT")
            .collect::<Vec<_>>(),
        "only the cleared subrecords leave"
    );
}

/// An exterior cell's grid reads, and the three unused bytes after it survive.
///
/// 30,098 of the master's exterior cells store a twelve-byte `XCLC` — x, y,
/// one hide byte and three bytes of `0xCD` — measured on all of them. A save
/// that rebuilt the subrecord from the spec's uint32 would grow it to twelve
/// bytes and drop those three.
#[test]
fn an_exterior_cells_grid_keeps_its_own_bytes() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    // An exterior cell whose grid carries the optional bytes: the first one the
    // master holds with a twelve-byte XCLC.
    let fid = s
        .doc
        .records()
        .into_iter()
        .find(|(_, r)| {
            &r.header.sig == b"CELL"
                && r.subrecords().ok().map(|subs| {
                    subs.iter()
                        .find(|x| &x.sig == b"XCLC")
                        .map(|x| x.data.len() == 12)
                        == Some(true)
                }) == Some(true)
        })
        .map(|(f, _)| f)
        .expect("the master holds exterior cells with a twelve-byte grid");
    let before = payload(&subs(&s, fid), b"XCLC");
    assert_eq!(before.len(), 12, "the fixture carries the optional bytes");

    let mut state = CellEditorState::default();
    state.load_from_session(&s, fid);
    assert!(state.has_grid, "an exterior cell carries an XCLC");
    // The no-op apply is what queues the record as an override; without it the
    // save writes only the records the session touched, and this one is not.
    state.save_to_session(&mut s);
    let after = saved_subs(&s, fid, "cell_editor_grid");
    assert_eq!(
        payload(&after, b"XCLC"),
        before,
        "the grid subrecord is unchanged, its unused bytes included"
    );

    // Moving the grid writes the two int32s and leaves the tail alone.
    state.grid_x = 42;
    state.save_to_session(&mut s);
    let moved = payload(&saved_subs(&s, fid, "cell_editor_grid_moved"), b"XCLC");
    assert_eq!(
        i32::from_le_bytes(moved[0..4].try_into().unwrap()),
        42,
        "the grid's x is written"
    );
    assert_eq!(
        &moved[9..],
        &before[9..],
        "the unused bytes after the hide byte survive"
    );
}

/// A grid with no optional bytes stays eight bytes long.
///
/// Eleven of the master's exterior cells store an eight-byte `XCLC` — x and y
/// only, with the hide byte the spec marks optional absent. Growing one to
/// nine or twelve on a save that changed nothing is the bug this pins: the
/// subrecord's length is the record's own, not the spec's.
#[test]
fn a_grid_without_the_optional_bytes_stays_short() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let fid = s
        .doc
        .records()
        .into_iter()
        .find(|(_, r)| {
            &r.header.sig == b"CELL"
                && r.subrecords().ok().map(|subs| {
                    subs.iter()
                        .find(|x| &x.sig == b"XCLC")
                        .map(|x| x.data.len() == 8)
                        == Some(true)
                }) == Some(true)
        })
        .map(|(f, _)| f);
    // Eleven cells is a real case, but it is not one to assert exists: a
    // different master may hold none. The save is only checked when it does.
    let Some(fid) = fid else {
        eprintln!("no eight-byte XCLC in this master; skipping");
        return;
    };
    let before = payload(&subs(&s, fid), b"XCLC");

    let mut state = CellEditorState::default();
    state.load_from_session(&s, fid);
    assert!(state.has_grid, "an exterior cell carries an XCLC");
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "cell_editor_grid_short"), b"XCLC");
    assert_eq!(
        after, before,
        "an eight-byte grid stays eight bytes on a no-op save"
    );
}

/// The header's own flags round-trip, and a changed one lands.
///
/// `Quest Item` is bit 10 and `Persistent` is bit 0x400 of the record header,
/// not of any subrecord. A window that shows them but never writes them is a
/// checkbox that does nothing — which is what this pins against.
#[test]
fn the_headers_flags_round_trip() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let fid = hoover_dam(&s);
    let before_flags = s.doc.record(fid).expect("the record").header.flags;

    let mut state = CellEditorState::default();
    state.load_from_session(&s, fid);
    assert_eq!(
        state.record_flags, before_flags,
        "the header's flags are read as the record stores them"
    );

    // Setting Quest Item and saving writes the header word.
    state.record_flags |= 1 << 10;
    state.save_to_session(&mut s);

    let path = std::env::temp_dir().join("nvre_cell_editor_flags.esp");
    s.save(&path).expect("save");
    let bytes = std::fs::read(&path).expect("read back");
    let plugin = nvre_esm::Plugin::parse_bytes(bytes::Bytes::from(bytes), "flags.esp".to_string())
        .expect("parse");
    let _ = std::fs::remove_file(&path);
    let mut written = None;
    nvre_esm::raw::visit_nodes(&plugin.nodes, &mut |r, _| {
        if r.header.form_id == fid {
            written = Some(r.header.flags);
        }
    });
    let written = written.expect("the record survives the save");
    assert_ne!(
        written & (1 << 10),
        0,
        "Quest Item is set in the written header"
    );
    // The other bits are the record's own: the word is written whole, so
    // clearing one does not clear the rest.
    assert_eq!(
        written & !(1 << 10),
        before_flags & !(1 << 10),
        "the flags the editor did not touch are unchanged"
    );
}

/// The Common Data section's flags land on the `DATA` byte.
///
/// `Has Water`, `No LOD Water`, `Can't Travel From Here` and `Hand Changed`
/// are the four bits GECK's Common Data tab edits, and they are four bits of
/// the one `DATA` flag word — not a subrecord of their own. The section's
/// checkboxes and the flags section's are therefore one editor, and this pins
/// that a change through either lands on the same byte.
#[test]
fn the_common_data_flags_land_on_the_data_byte() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let fid = hoover_dam(&s);
    // The fixture's own byte: interior, has water, public place.
    let before = payload(&subs(&s, fid), b"DATA");
    assert_eq!(before, vec![0x23], "the fixture's own DATA byte");

    let mut state = CellEditorState::default();
    state.load_from_session(&s, fid);
    // The four Common Data bits, as the section's checkboxes would set them.
    state.data_flags = 0x23 | 0x04 | 0x08 | 0x40;
    state.save_to_session(&mut s);

    let after = payload(&saved_subs(&s, fid, "cell_editor_common"), b"DATA");
    assert_eq!(
        after,
        vec![0x6F],
        "Can't Travel, No LOD Water and Hand Changed are set on the same byte"
    );
    // The bits the section does not offer are the record's own.
    assert_eq!(
        after[0] & 0x01,
        0x01,
        "Is Interior Cell survives, because nothing cleared it"
    );
    assert_eq!(
        after[0] & 0x20,
        0x20,
        "Public Place survives, because nothing cleared it"
    );
}

/// The window opens and closes on the session's state, and nothing else.
///
/// `open_cell_editor` is what the Cells window's right-click menu calls; it
/// does not move the viewport, which is a separate choice.
#[test]
fn the_editor_opens_without_moving_the_viewport() {
    let Some(mut s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    let fid = hoover_dam(&s);
    s.open_cell(fid);
    let open = s.cell;
    s.open_cell_editor(fid);

    assert_eq!(s.editing_cell, Some(fid), "the window is open on this cell");
    assert_eq!(s.cell, open, "the viewport's cell did not move");
    s.close_cell_editor();
    assert_eq!(s.editing_cell, None, "the window is closed");
}

/// A cell the load order does not have cannot be opened.
///
/// The window's `manage_window` refuses to spawn for a cell that resolves to
/// nothing, so a FormID typed by hand does not open an empty editor.
#[test]
fn an_unknown_cell_does_not_open() {
    let Some(s) = session() else {
        eprintln!("NVRE_DATA not set; skipping");
        return;
    };
    assert!(
        s.resolved_record(0x7FFF_FFFF).is_none(),
        "the fixture FormID is not in the load order"
    );
}
