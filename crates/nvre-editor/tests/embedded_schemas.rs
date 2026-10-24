//! The schemas compiled into the binary.
//!
//! A built editor is one file the user copies somewhere, so the schemas
//! cannot live on the disk beside it. They are compiled in instead, and this
//! checks that the compiled-in schemas are the same schemas the repository
//! holds — a layout read from the binary and a layout read from the
//! directory have to be the same layout, or the editor a user runs is not the
//! editor the developer tested.

use nvre_editor::game::{self, GameProfile};

/// The schemas on the disk, for one game.
///
/// The chain is the base, then the game's family, then the game's own
/// version — the same chain the compiled-in schemas are built from.
fn from_disk(game: &GameProfile) -> nvre_esm::Schema {
    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas"));
    let mut layers: Vec<String> = Vec::new();
    for name in [game.schema_family_dir, game.schema_version_dir] {
        if name.is_empty() {
            continue;
        }
        let dir = root.join(name);
        if dir.is_dir() {
            layers.push(dir.to_string_lossy().into_owned());
        }
    }
    nvre_esm::Schema::load_game(root.join("base"), &layers).expect("the schemas load")
}

/// The schemas compiled into the binary, for one game.
fn from_binary(game: &GameProfile) -> nvre_esm::Schema {
    game.load_schema().expect("the compiled-in schemas load")
}

/// Everything a schema holds, as a comparable value.
///
/// The records, the types and the object-window table together: a schema
/// that differs in any of them decodes differently, so all three are
/// compared.
fn fingerprint(schema: &nvre_esm::Schema) -> String {
    let records: Vec<String> = schema
        .records
        .iter()
        .map(|(sig, def)| format!("{sig}: {:?}", def.subrecords))
        .collect();
    let types: Vec<String> = schema
        .types
        .iter()
        .map(|(name, def)| format!("{name}: {def:?}"))
        .collect();
    format!(
        "records: {}\ntypes: {}\nwindow: {:?}",
        records.join("\n"),
        types.join("\n"),
        schema.object_window
    )
}

#[test]
fn every_game_compiles_its_schemas_in() {
    for game in &game::ALL {
        let schema = from_binary(game);
        assert!(
            !schema.records.is_empty(),
            "{}: no record types were compiled in",
            game.id
        );
    }
}

#[test]
fn the_compiled_in_schemas_match_the_ones_on_disk() {
    for game in &game::ALL {
        let disk = fingerprint(&from_disk(game));
        let binary = fingerprint(&from_binary(game));
        assert_eq!(
            disk, binary,
            "{}: the compiled-in schemas differ from the ones on disk",
            game.id
        );
    }
}

#[test]
fn the_base_layer_is_compiled_in() {
    // The base is what every game shares, and it is the layer a game with no
    // layouts of its own reads alone.
    let base = nvre_schemas::base();
    assert!(
        !base.files.is_empty(),
        "the base layer holds no schema files"
    );
    let on_disk = nvre_esm::Schema::load_dir(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../schemas/base"
    )))
    .expect("the base loads");
    for game in &game::ALL {
        let schema = from_binary(game);
        assert!(
            schema.records.len() >= on_disk.records.len(),
            "{}: the compiled-in base holds fewer records than the base on disk",
            game.id
        );
    }
}

#[test]
fn a_game_directory_reads_its_base_without_a_symlink() {
    // A Windows checkout without symlink support writes the `base` entry as a
    // small text file holding the target's path, so a game directory is read
    // without the base it carries. The base is beside it, and that is where
    // it is found.
    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas"));
    let base_src = root.join("base");
    let game_src = root.join("fnv");

    // A copy of the two directories, with the game's `base` entry written as
    // a text file rather than a link, which is what git does on a Windows
    // checkout without symlink support.
    let tmp = std::env::temp_dir().join("nvre-base-symlink-test");
    let _ = std::fs::remove_dir_all(&tmp);
    let base = tmp.join("base");
    let game = tmp.join("fnv");
    copy_dir(&base_src, &base);
    copy_dir(&game_src, &game);
    // The link, as a text file.
    let linked = game.join("base");
    let _ = std::fs::remove_dir_all(&linked);
    std::fs::write(&linked, "../base").expect("the link is written");

    let schema = nvre_esm::Schema::load_dir(&game).expect("the game directory loads");
    assert!(
        !schema.records.is_empty(),
        "a game directory read without its base holds no records"
    );
    // The base's own records are there, which is the point: the base was
    // found beside the game directory.
    let base_only = nvre_esm::Schema::load_dir(&base).expect("the base loads");
    for sig in base_only.records.keys() {
        assert!(
            schema.records.contains_key(sig),
            "{sig} is in the base but not in the game directory that carries it"
        );
    }
}

/// Copy a directory's files, without its links.
fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("the directory is created");
    for entry in std::fs::read_dir(from)
        .expect("the directory reads")
        .flatten()
    {
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).expect("the file copies");
        }
    }
}
