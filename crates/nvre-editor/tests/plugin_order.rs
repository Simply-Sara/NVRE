//! Reading `plugins.txt` / `loadorder.txt`, and ordering a master list by one.
//!
//! These are text-file rules and a sort, so nothing here needs the game
//! installed: the order files are written into a scratch folder, and the master
//! ordering is exercised on the graph itself.
//!
//! A note on what these files *mean*, because it changes what a test may claim.
//! For Fallout 3, Fallout: New Vegas and Oblivion the engine orders plugins by
//! file timestamp and `plugins.txt` only records which are active [documented:
//! LOOT, "Introduction to Load Orders"]. So a test here may assert that the file
//! was read and applied, and must not assert that the game will load in that
//! order — that is not what the file decides.

use nvre_editor::plugin_order::{self, PluginOrder, Rank};

/// A scratch folder with `files` written into it, by name.
fn folder(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nvre-plugin-order-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

/// The plugin names a folder holds, as `Launcher::refresh_listing` would see
/// them: the folder's plugins, sorted by name.
fn listing(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|x| {
                    x.eq_ignore_ascii_case("esm")
                        || x.eq_ignore_ascii_case("esp")
                        || x.eq_ignore_ascii_case("esl")
                })
                .unwrap_or(false)
        })
        .collect();
    out.sort();
    out
}

/// The names in a listing, for asserting on.
fn names(paths: &[std::path::PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// One plugin per line, in the file's order, is the order the list takes.
#[test]
fn an_order_file_reorders_the_list() {
    let dir = folder(
        "reorder",
        &[
            ("plugins.txt", "b.esm\na.esm\nc.esm\n"),
            ("a.esm", ""),
            ("b.esm", ""),
            ("c.esm", ""),
        ],
    );

    let file = plugin_order::read(&dir, PluginOrder::PluginsTxt).expect("plugins.txt is there");
    assert_eq!(file.names, vec!["b.esm", "a.esm", "c.esm"]);

    let mut list = listing(&dir);
    assert_eq!(names(&list), vec!["a.esm", "b.esm", "c.esm"]);
    plugin_order::order_paths(&mut list, &Rank::of(&file.names));
    assert_eq!(names(&list), vec!["b.esm", "a.esm", "c.esm"]);
}

/// A plugin the file does not name is kept, after the ones it does.
///
/// A Data folder holds plugins an order file has never heard of — an inactive
/// `.esp`, a master installed since the game last ran — and a listing that
/// dropped them would hide them from the user.
#[test]
fn plugins_the_file_does_not_name_come_last() {
    let dir = folder(
        "unranked",
        &[
            ("loadorder.txt", "z.esm\n"),
            ("a.esm", ""),
            ("m.esm", ""),
            ("z.esm", ""),
        ],
    );

    let file = plugin_order::read(&dir, PluginOrder::LoadOrderTxt).unwrap();
    let mut list = listing(&dir);
    plugin_order::order_paths(&mut list, &Rank::of(&file.names));
    assert_eq!(
        names(&list),
        vec!["z.esm", "a.esm", "m.esm"],
        "the named plugin first, then the rest in file-name order"
    );
}

/// A file that is not there is `None`, not an empty order.
///
/// The two are different answers: no file means the folder has no order to read,
/// an empty file means the order file exists and names nothing. A caller that
/// could not tell them apart would report a folder with no `plugins.txt` as a
/// folder whose `plugins.txt` is empty.
#[test]
fn a_missing_file_is_none_and_an_empty_one_is_empty() {
    let dir = folder("missing", &[("plugins.txt", ""), ("a.esm", "")]);
    assert!(plugin_order::read(&dir, PluginOrder::LoadOrderTxt).is_none());
    let empty = plugin_order::read(&dir, PluginOrder::PluginsTxt).unwrap();
    assert!(empty.names.is_empty());
    assert_eq!(PluginOrder::Name.file(), None);
}

/// The comments, blank lines and `*` markers the formats carry are skipped.
///
/// The `*` is how the games from Skyrim Special Edition on mark an active
/// plugin [documented: the asterisk format, `*` = active]; a parser that kept it
/// would match no file in the folder, and every plugin would fall to the end of
/// the list.
#[test]
fn comments_blanks_and_asterisks_are_skipped() {
    let dir = folder(
        "comments",
        &[
            (
                "plugins.txt",
                "# a comment\n\n*a.esm\n   b.esm  \n\n# another\n",
            ),
            ("a.esm", ""),
            ("b.esm", ""),
        ],
    );

    let file = plugin_order::read(&dir, PluginOrder::PluginsTxt).unwrap();
    assert_eq!(file.names, vec!["a.esm", "b.esm"]);
}

/// A byte Windows-1252 has and UTF-8 does not still reads as text.
///
/// `plugins.txt` is Windows-1252 and `loadorder.txt` is UTF-8 [documented: the
/// encodings mod managers' own documentation states], and 0xE9 is `é` in the
/// first and not valid UTF-8 on its own — so a file with an accented name in it
/// is the case that tells the two decoders apart.
#[test]
fn a_windows_1252_name_decodes() {
    let dir = folder("cp1252", &[]);
    let mut text = b"caf".to_vec();
    text.push(0xE9);
    text.extend_from_slice(b".esm\n");
    std::fs::write(dir.join("plugins.txt"), &text).unwrap();
    std::fs::write(dir.join("caf\u{e9}.esm"), b"").unwrap();

    let file = plugin_order::read(&dir, PluginOrder::PluginsTxt).unwrap();
    assert_eq!(file.names, vec!["caf\u{e9}.esm"]);

    let mut list = listing(&dir);
    plugin_order::order_paths(&mut list, &Rank::of(&file.names));
    assert_eq!(names(&list), vec!["caf\u{e9}.esm"]);
}

/// The master list takes the file's order, and the headers still win where they
/// conflict.
///
/// This is the rule that keeps a load order loadable. `masters_in` returns a
/// topological sort of the `MAST` headers, and the file's order is applied
/// *inside* that sort — so `FalloutNV.esm` is named last by this file and still
/// comes first, because three plugins require it. Re-sorting the topological
/// result by the file instead would give `MasterAfterDependent`, which
/// `LoadOrder::new` rejects: a list the game cannot load, produced by a file
/// that merely ordered things differently.
#[test]
fn masters_take_the_file_order_without_breaking_the_headers() {
    let present: Vec<String> = [
        "FalloutNV.esm",
        "DeadMoney.esm",
        "HonestHearts.esm",
        "ModA.esm",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // Both DLC require the base; ModA requires DeadMoney. The two DLC constrain
    // nothing against each other, which is the pair a file can reorder.
    let needs: Vec<(usize, Vec<usize>)> =
        vec![(0, vec![]), (1, vec![0]), (2, vec![0]), (3, vec![1])];
    let game = &nvre_editor::game::FNV;

    // No order file: the base first (most dependents), then the DLC by name,
    // then the plugin that needs one of them.
    let topo = game.order_masters(&present, &needs, &Rank::default());
    assert_eq!(
        topo,
        vec![
            "FalloutNV.esm",
            "DeadMoney.esm",
            "HonestHearts.esm",
            "ModA.esm"
        ],
        "the headers alone put the base first and the unconstrained DLC by name"
    );

    // A file that names the two unconstrained DLC the other way round. Both are
    // ready at the same moment, so the file's order decides between them.
    let rank = Rank::of(&["HonestHearts.esm".to_string(), "DeadMoney.esm".to_string()]);
    assert_eq!(
        game.order_masters(&present, &needs, &rank),
        vec![
            "FalloutNV.esm",
            "HonestHearts.esm",
            "DeadMoney.esm",
            "ModA.esm"
        ],
        "the headers are satisfied and the file's preference is honoured"
    );

    // A file that names them in the worst possible order cannot have it: the
    // base is the only master ready at the first step, because everything else
    // requires it, so naming it last changes nothing. The result is the same as
    // the friendly file's.
    let hostile = Rank::of(&[
        "ModA.esm".to_string(),
        "HonestHearts.esm".to_string(),
        "FalloutNV.esm".to_string(),
        "DeadMoney.esm".to_string(),
    ]);
    assert_eq!(
        game.order_masters(&present, &needs, &hostile),
        vec![
            "FalloutNV.esm",
            "HonestHearts.esm",
            "DeadMoney.esm",
            "ModA.esm"
        ],
        "the headers are a constraint the file cannot override"
    );
}

/// A master the file does not name is not dropped, and one the folder lacks is
/// not invented.
///
/// The file is a claim about order, not a listing of the folder: `plugins.txt`
/// in Fallout: New Vegas's prefix names 19 plugins while the Data folder holds
/// more, and a `loadorder.txt` from another install may name plugins that are
/// not here at all.
#[test]
fn the_master_list_is_the_folders_not_the_files() {
    let present: Vec<String> = ["FalloutNV.esm", "DeadMoney.esm"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let needs: Vec<(usize, Vec<usize>)> = vec![(0, vec![]), (1, vec![0])];
    let game = &nvre_editor::game::FNV;

    // A file naming a plugin that is not installed changes nothing.
    let rank = Rank::of(&["NotInstalled.esm".to_string(), "DeadMoney.esm".to_string()]);
    assert_eq!(
        game.order_masters(&present, &needs, &rank),
        vec!["FalloutNV.esm", "DeadMoney.esm"],
        "the list is the folder's plugins, and FalloutNV is unnamed so it stays"
    );
}

/// Reading a real plugin's header agrees with reading the whole file.
///
/// `masters_in_header` exists to answer `Plugin::open().masters()` for a tenth
/// of the cost, so the two must agree on real files — the whole point is that it
/// is the cheap answer to the same question. The fixtures are GECK's own, and
/// two of them declare no master at all.
#[test]
fn the_header_reader_agrees_with_the_whole_file_reader() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testfiles");
    let mut checked = 0;
    for name in [
        "fonv_geck_empty.esm",
        "fonv_geck_empty.esp",
        "fonv_geck_add_weap.esm",
        "fonv_geck_author_desc.esm",
        "fonv_geck_empty_no_master.esm",
        "fonv_geck_empty_no_master.esp",
    ] {
        let path = dir.join(name);
        let Some(whole) = nvre_esm::Plugin::open(&path).ok() else {
            continue;
        };
        let header = nvre_esm::Plugin::masters_in_header(&path)
            .unwrap_or_else(|| panic!("{name} has a TES4 header"));
        assert_eq!(header, whole.masters(), "{name}: header vs whole file");
        checked += 1;
    }
    assert!(checked > 0, "no testfile was readable at {}", dir.display());
}
