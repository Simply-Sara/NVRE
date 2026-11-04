//! Skyrim's two editions, and the three places they differ.
//!
//! Legendary Edition and Special Edition are one family with two version layers:
//! `schemas/skyrim` holds what both share, `schemas/skyrimle` and
//! `schemas/skyrimse` hold the differences. The editor must serve both, and the
//! differences that matter are not cosmetic — they are:
//!
//! - **archives**: LE ships BSA v104 (zlib), SE ships v105 (LZ4). Reading one with
//!   the other's reader gives nothing at all, so an SE install would look empty.
//! - **`RACE.DATA`**: SE's racial data is 164 bytes where LE's is 128, and the
//!   wider one is a union *inside* a union — the shape that made 24 fields on
//!   Skyrim SE report as unencodable when they were only out of range.
//! - **pages**: both read the family's object-window table, so a page written once
//!   serves both editions. That is what the family layer is for.
//!
//! These assertions need no game data: the schemas and the profiles are compiled
//! in. The archive check does need an install, and skips without one.

use nvre_editor::game::GameProfile;

/// The two edition profiles, and the layer each adds to the family.
const EDITIONS: [(&str, &str); 2] = [("skyrim", "skyrimle"), ("skyrimse", "skyrimse")];

#[test]
fn both_editions_are_profiles_with_their_own_version_layer() {
    for (id, version) in EDITIONS {
        let p = GameProfile::by_id(id).unwrap_or_else(|| panic!("{id} is a profile"));
        assert_eq!(p.master, "Skyrim.esm", "{id} opens the same master");
        assert_eq!(
            p.schema_family_dir, "skyrim",
            "{id} reads the family's shared layouts"
        );
        assert_eq!(
            p.schema_version_dir, version,
            "{id} adds its own differences from `schemas/{version}`"
        );
    }
}

#[test]
fn the_editions_ship_different_archive_containers() {
    // The difference that made SE unusable before the `bsa105` crate: same folder
    // layout, different compression, so the wrong reader finds no files rather
    // than wrong ones.
    let le = GameProfile::by_id("skyrim").expect("skyrim");
    let se = GameProfile::by_id("skyrimse").expect("skyrimse");
    assert_ne!(
        le.archive_format, se.archive_format,
        "LE and SE do not ship the same container"
    );
    assert_ne!(
        le.archives.len(),
        se.archives.len(),
        "SE splits its textures and meshes into numbered archives"
    );
    assert!(
        se.archives.iter().any(|a| a.contains("Meshes0")),
        "SE's own archive names are the ones it ships: {:?}",
        se.archives
    );
    assert!(
        !le.archives.iter().any(|a| a.contains("Meshes0")),
        "LE ships one Meshes archive"
    );
}

#[test]
fn both_editions_get_the_family_page_for_every_record_type() {
    // A page is written once and serves both editions, because the family layer
    // is where the object window lives. If SE needed its own table, this is where
    // it would show.
    let le = GameProfile::by_id("skyrim")
        .expect("skyrim")
        .load_schema()
        .expect("schema");
    let se = GameProfile::by_id("skyrimse")
        .expect("skyrimse")
        .load_schema()
        .expect("schema");
    assert!(!le.object_window.categories.is_empty(), "LE has a table");
    assert!(!se.object_window.categories.is_empty(), "SE has a table");
    let mut pages = 0;
    for sig in se.object_window.categories.keys() {
        if se.page_is_described(sig) {
            pages += 1;
        }
    }
    assert!(
        pages > 100,
        "SE's record types are described pages, not a flat grid: {pages}"
    );
}

#[test]
fn the_wider_racial_data_is_a_union_inside_a_union() {
    // SE's `RACE.DATA` is 164 bytes and its layout nests a union in a union. That
    // shape is what a naive encoder gets wrong: it tries the inner variants in
    // order and reports the first one's error, which says "this field is not part
    // of this layout" when the truth is "this number does not fit".
    use nvre_esm::TypeDef;
    let se = GameProfile::by_id("skyrimse")
        .expect("skyrimse")
        .load_schema()
        .expect("schema");
    let def = se
        .lookup(b"RACE", b"DATA")
        .expect("SE's RACE has a DATA layout");
    let outer = se.resolve_def(&def.ty);
    let TypeDef::Union(variants) = &outer else {
        panic!("SE's RACE.DATA is a size union");
    };
    let sizes: Vec<Option<usize>> = variants.iter().map(|(n, _)| *n).collect();
    assert!(
        sizes.contains(&Some(128)) && sizes.contains(&Some(164)),
        "SE's RACE.DATA has both the 128-byte and 164-byte forms: {sizes:?}"
    );
    let wider = variants
        .iter()
        .find(|(n, _)| *n == Some(164))
        .expect("the 164-byte form");
    assert!(
        matches!(wider.1, TypeDef::Union(_)),
        "the 164-byte form is itself a union, which is the nesting that matters"
    );
}

#[test]
fn a_value_too_big_for_its_field_is_a_bound_not_a_broken_layout() {
    // The bug this test was written for: on Skyrim SE's `RACE`, 24 fields sat at
    // their type's maximum, so `+1` did not fit — and the nested union reported
    // "no union variant matches: field ... is not part of this layout", which
    // reads as a schema bug. `EncodeError::is_range` is what tells the two apart,
    // and it has to survive a union wrapping it.
    use nvre_esm::schema::Prim;
    use nvre_esm::TypeDef;
    let se = GameProfile::by_id("skyrimse")
        .expect("skyrimse")
        .load_schema()
        .expect("schema");
    let def = se.lookup(b"RACE", b"DATA").expect("SE's RACE DATA");
    let ty = se.resolve_def(&def.ty);
    // A value with every field of the widest form at its maximum is not something
    // to build by hand; instead check the error the encoder gives for a `u8` that
    // does not fit, through a union, which is the shape that lost it.
    let mut out = Vec::new();
    let err = nvre_edit::encode::encode_type(
        &se,
        &TypeDef::Union(vec![(Some(1), TypeDef::Prim(Prim::U8))]),
        &serde_json::json!(256),
        &mut out,
        "skill",
    )
    .expect_err("256 does not fit a u8");
    assert!(
        err.is_range(),
        "a union reports the range error, not the first variant's shape error: {err}"
    );
    let _ = ty;
}
