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

/// The Windows layout: the order file is under `%LOCALAPPDATA%`, not in `Data`.
///
/// On Windows the game keeps `plugins.txt` in `%LOCALAPPDATA%\<game>\` beside
/// its INIs and saves — the Data folder holds plugins, not the user's order
/// [documented: LOOT, "Introduction to Load Orders"]. A launcher that searched
/// only `Data` would therefore never find the real file on the platform most of
/// these games are played on.
///
/// `LOCALAPPDATA` is read from the environment, so the layout is reproduced
/// here on any platform by pointing that variable at a scratch folder. The test
/// is serialised against the other environment-reading test in this file by
/// running single-threaded, which is how CI runs the workspace.
#[test]
fn the_windows_localappdata_location_is_searched() {
    let root = std::env::temp_dir().join("nvre-plugin-order-localappdata");
    let _ = std::fs::remove_dir_all(&root);
    // `%LOCALAPPDATA%\FalloutNV\plugins.txt`, and a Data folder that holds the
    // plugins but no order file.
    let local = root.join("AppData").join("Local");
    let game_dir = local.join("FalloutNV");
    let data = root.join("Fallout New Vegas").join("Data");
    std::fs::create_dir_all(&game_dir).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(game_dir.join("plugins.txt"), "b.esm\na.esm\n").unwrap();
    for name in ["a.esm", "b.esm"] {
        std::fs::write(data.join(name), b"").unwrap();
    }

    // The variable is process-wide, so it is restored before this test returns
    // rather than left for whatever runs next.
    let previous = std::env::var_os("LOCALAPPDATA");
    unsafe { std::env::set_var("LOCALAPPDATA", &local) };

    let game = &nvre_editor::game::FNV;
    let found = plugin_order::read_for_game(game, &data, PluginOrder::PluginsTxt)
        .expect("the file under LOCALAPPDATA must be found");
    assert_eq!(found.path, game_dir.join("plugins.txt"));
    assert_eq!(found.names, vec!["b.esm", "a.esm"]);

    // And the Data folder is still searched, which is the only place a portable
    // install has it.
    std::fs::write(data.join("loadorder.txt"), "a.esm\nb.esm\n").unwrap();
    let from_data = plugin_order::read_for_game(game, &data, PluginOrder::LoadOrderTxt)
        .expect("the Data folder must still be searched");
    assert_eq!(from_data.path, data.join("loadorder.txt"));

    // LocalAppData wins when both have the file: it is where the game reads
    // from, so the user's actual order is the one that must apply.
    std::fs::write(game_dir.join("loadorder.txt"), "b.esm\na.esm\n").unwrap();
    let both = plugin_order::read_for_game(game, &data, PluginOrder::LoadOrderTxt).unwrap();
    assert_eq!(
        both.path,
        game_dir.join("loadorder.txt"),
        "the game's own location wins over a copy in Data"
    );

    match previous {
        Some(v) => unsafe { std::env::set_var("LOCALAPPDATA", v) },
        None => unsafe { std::env::remove_var("LOCALAPPDATA") },
    }
}

/// A file picked by hand replaces the search rather than joining it.
///
/// The point of the launcher's `Browse…` button is to order by a list that is
/// *not* where the game keeps one — a mod manager's profile, a backup, a list
/// kept by hand. If a discovered file could override the picked one, picking it
/// would mean nothing, so the picked path is read directly and the search is
/// skipped.
#[test]
fn a_picked_file_replaces_the_search() {
    let root = std::env::temp_dir().join("nvre-plugin-order-picked");
    let _ = std::fs::remove_dir_all(&root);
    let data = root.join("Data");
    let elsewhere = root.join("profile");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();

    // The folder has its own order file, and the user picks a different one.
    std::fs::write(data.join("plugins.txt"), "a.esm\nb.esm\n").unwrap();
    let picked = elsewhere.join("my-order.txt");
    std::fs::write(&picked, "b.esm\na.esm\n").unwrap();

    let from_data =
        plugin_order::read_for_game(&nvre_editor::game::FNV, &data, PluginOrder::PluginsTxt)
            .expect("the Data folder's own file");
    assert_eq!(from_data.names, vec!["a.esm", "b.esm"]);

    let from_pick = plugin_order::read_file(&picked).expect("the picked file");
    assert_eq!(from_pick.names, vec!["b.esm", "a.esm"]);
    assert_eq!(from_pick.path, picked);

    // A picked file is not required to be named like the order it stands in
    // for: `my-order.txt` orders a `plugins.txt` list.
    assert_eq!(from_pick.path.file_name().unwrap(), "my-order.txt");
}

/// A picked file that cannot be read is a failure, not a silent fallback.
///
/// Falling back to the search would show a list the user did not ask for while
/// the picker's own choice appeared to be in force. The launcher keeps the error
/// so it can say so.
#[test]
fn an_unreadable_pick_is_none() {
    let missing = std::env::temp_dir().join("nvre-plugin-order-does-not-exist.txt");
    let _ = std::fs::remove_file(&missing);
    assert!(plugin_order::read_file(&missing).is_none());
}

/// A picked file obeys the same format rules as a discovered one.
///
/// The format does not know where the file came from: comments, blank lines and
/// the `*` active marker are handled identically, because one parser reads both.
#[test]
fn a_picked_file_is_parsed_like_any_other() {
    let root = std::env::temp_dir().join("nvre-plugin-order-picked-format");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let picked = root.join("exported.txt");
    std::fs::write(
        &picked,
        "# exported from a mod manager\n\n*Mod.esm\nOther.esp\n",
    )
    .unwrap();

    let file = plugin_order::read_file(&picked).unwrap();
    assert_eq!(file.names, vec!["Mod.esm", "Other.esp"]);
}

/// The header summary reads the author and description, not only the masters.
///
/// The launcher's Open Existing tab shows all three above the master list, and
/// they are three subrecords of the same `TES4` record — so one walk has to
/// yield all of them. The plugin is written by the editor, which is the only
/// fixture available: `testfiles/` is not in the repository.
#[test]
fn the_header_summary_carries_author_and_description() {
    let dir = std::env::temp_dir().join("nvre-plugin-order-summary");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let doc = nvre_edit::Document::create_empty(
        "Authored.esp",
        false,
        &["FalloutNV.esm".to_string(), "DeadMoney.esm".to_string()],
        Some("Someone"),
        Some("what this plugin is for"),
    )
    .unwrap();
    let path = dir.join("Authored.esp");
    doc.save(&path).unwrap();

    let summary = nvre_esm::Plugin::header_summary(&path).expect("a TES4 header");
    assert_eq!(summary.masters, vec!["FalloutNV.esm", "DeadMoney.esm"]);
    assert_eq!(summary.author.as_deref(), Some("Someone"));
    assert_eq!(
        summary.description.as_deref(),
        Some("what this plugin is for")
    );

    // And it agrees with reading the whole file, which is the claim that makes
    // the cheap reader safe to use in place of it.
    let whole = nvre_esm::Plugin::open(&path).unwrap();
    assert_eq!(summary.masters, whole.masters());
    assert_eq!(summary.author, whole.header.author);
    assert_eq!(summary.description, whole.header.description);

    // An empty description is no `SNAM`, not an empty one: `None` here, so the
    // panel shows "(none)" rather than a blank line.
    let bare = nvre_edit::Document::create_empty("Bare.esp", false, &[], None, None).unwrap();
    let bare_path = dir.join("Bare.esp");
    bare.save(&bare_path).unwrap();
    let bare_summary = nvre_esm::Plugin::header_summary(&bare_path).unwrap();
    assert!(bare_summary.masters.is_empty());
    assert_eq!(bare_summary.description, None);
    // GECK's own default author is written by `create_empty`, so this is the
    // string a plugin nobody claimed carries.
    assert_eq!(bare_summary.author.as_deref(), Some("DEFAULT"));
}

/// A plugin's master list is the masters its own header declares, in load
/// order, and nothing else.
///
/// This is what the Open Existing tab shows: the file's own `MAST` names, put
/// in the order the game would load them. A folder holding other masters must
/// not add them — that was the checkboxes' behaviour, and it let a session open
/// with masters the plugin does not declare.
#[test]
fn the_master_list_is_the_files_own() {
    let root = std::env::temp_dir().join("nvre-plugin-order-files-own");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    // Four masters installed; the plugin declares two of them, in the reverse
    // of load order. `DeadMoney` declares `FalloutNV` the way a real DLC does,
    // which is what makes the load order a fact rather than a preference.
    let base: Vec<String> = Vec::new();
    let on_base = vec!["FalloutNV.esm".to_string()];
    for (name, masters) in [
        ("FalloutNV.esm", &base),
        ("DeadMoney.esm", &on_base),
        ("HonestHearts.esm", &on_base),
        ("Unrelated.esm", &base),
    ] {
        let doc = nvre_edit::Document::create_empty(name, true, masters, None, None).unwrap();
        doc.save(root.join(name)).unwrap();
    }
    let declared = vec!["DeadMoney.esm".to_string(), "FalloutNV.esm".to_string()];
    let plugin =
        nvre_edit::Document::create_empty("Mod.esp", false, &declared, None, None).unwrap();
    let plugin_path = root.join("Mod.esp");
    plugin.save(&plugin_path).unwrap();

    let summary = nvre_esm::Plugin::header_summary(&plugin_path).unwrap();
    let ordered = nvre_editor::game::order_named(
        &summary.masters,
        &root,
        &nvre_editor::plugin_order::Rank::default(),
    );

    assert_eq!(
        ordered,
        vec!["FalloutNV.esm", "DeadMoney.esm"],
        "the two declared masters, base first because DeadMoney requires it"
    );
    assert!(
        !ordered
            .iter()
            .any(|m| m == "HonestHearts.esm" || m == "Unrelated.esm"),
        "a master the plugin does not declare must not appear"
    );
}

/// A declared master that is not installed stays in the list, at the end.
///
/// It is what the file says, and hiding it would make the plugin look openable
/// when it is not: the session would fail on the missing master with no
/// indication of which one was absent.
#[test]
fn a_missing_master_is_listed_not_hidden() {
    let root = std::env::temp_dir().join("nvre-plugin-order-missing-master");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let doc = nvre_edit::Document::create_empty("FalloutNV.esm", true, &[], None, None).unwrap();
    doc.save(root.join("FalloutNV.esm")).unwrap();

    let declared = vec!["FalloutNV.esm".to_string(), "NotInstalled.esm".to_string()];
    let ordered = nvre_editor::game::order_named(
        &declared,
        &root,
        &nvre_editor::plugin_order::Rank::default(),
    );
    assert_eq!(
        ordered,
        vec!["FalloutNV.esm", "NotInstalled.esm"],
        "the installed master first, the missing one kept after it"
    );
}

/// Reading a real plugin's header agrees with reading the whole file.
///
/// `masters_in_header` exists to answer `Plugin::open().masters()` for a tenth
/// of the cost, so the two must agree — the whole point is that it is the cheap
/// answer to the same question, and an answer that differs is worse than a slow
/// one. The plugins are written by the editor itself rather than read from
/// `testfiles/`: that directory is not part of the repository (`*.esm` is
/// ignored), so a test that read it would pass here and fail everywhere else.
///
/// The three cases are the ones a header reader can get wrong: one master, no
/// masters, and a master list long enough to cross a subrecord boundary.
#[test]
fn the_header_reader_agrees_with_the_whole_file_reader() {
    let dir = std::env::temp_dir().join("nvre-plugin-order-header");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let cases: [(&str, &[&str]); 3] = [
        ("OneMaster.esm", &["FalloutNV.esm"]),
        ("NoMasters.esm", &[]),
        (
            "ManyMasters.esm",
            &[
                "FalloutNV.esm",
                "DeadMoney.esm",
                "HonestHearts.esm",
                "OldWorldBlues.esm",
                "LonesomeRoad.esm",
                "GunRunnersArsenal.esm",
                "ClassicPack.esm",
                "MercenaryPack.esm",
                "TribalPack.esm",
                "CaravanPack.esm",
            ],
        ),
    ];

    for (name, masters) in cases {
        let masters: Vec<String> = masters.iter().map(|m| m.to_string()).collect();
        let doc = nvre_edit::Document::create_empty(name, true, &masters, Some("NVRE"), None)
            .expect("a blank plugin is a plugin");
        let path = dir.join(name);
        doc.save(&path).expect("written to the scratch folder");

        let whole = nvre_esm::Plugin::open(&path).expect("the file it just wrote is a plugin");
        let header = nvre_esm::Plugin::masters_in_header(&path)
            .unwrap_or_else(|| panic!("{name} has a TES4 header"));
        assert_eq!(
            header,
            whole.masters(),
            "{name}: the header reader and the whole-file reader must agree"
        );
        assert_eq!(
            header, masters,
            "{name}: and both must be the masters the plugin was made with"
        );
    }

    // A file that is not a plugin is not read as one, rather than yielding
    // nonsense names: the header reader's whole job is to be safe to call on
    // every file in a Data folder.
    let not_a_plugin = dir.join("readme.txt");
    std::fs::write(&not_a_plugin, b"this is not a plugin\n").unwrap();
    assert_eq!(nvre_esm::Plugin::masters_in_header(&not_a_plugin), None);
    assert_eq!(
        nvre_esm::Plugin::masters_in_header(dir.join("NotThere.esm")),
        None
    );
}

/// S0-3: every game's own order file is found, and the master ordering the
/// game itself would produce is the one computed here.
///
/// The games differ in where they keep the file and in what decides the order —
/// Fallout 3, New Vegas and Oblivion order by file timestamp and use
/// `plugins.txt` only to record what is active, while Skyrim and Fallout 4 read
/// the file's own order — so each profile is exercised with its own file, and
/// the ordering rule is asserted on the dependency graph rather than on the
/// file. [documented: LOOT, "Introduction to Load Orders"]
#[test]
fn every_game_finds_its_own_order_file() {
    for game in nvre_editor::game::ALL {
        let dir = std::env::temp_dir().join(format!("nvre-order-{}", game.id));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // The file each game reads, with a `*` as Skyrim SE onward marks an
        // active plugin — a leading star must be dropped, not kept in the name.
        std::fs::write(dir.join("plugins.txt"), "*base.esm\nsecond.esm\n").unwrap();
        std::fs::write(dir.join("loadorder.txt"), "base.esm\nsecond.esm\n").unwrap();

        let from_txt = plugin_order::read_for_game(&game, &dir, PluginOrder::PluginsTxt)
            .unwrap_or_else(|| panic!("{}: plugins.txt", game.id));
        assert_eq!(
            from_txt.names,
            vec!["base.esm", "second.esm"],
            "{}: the names as the file lists them, star dropped",
            game.id
        );
        // `read_for_game` reports where it found the file rather than implying
        // the Data folder, and on Linux that is the only place there is.
        assert_eq!(
            from_txt.path.parent(),
            Some(dir.as_path()),
            "{}: the file it read",
            game.id
        );

        let from_load = plugin_order::read_for_game(&game, &dir, PluginOrder::LoadOrderTxt)
            .unwrap_or_else(|| panic!("{}: loadorder.txt", game.id));
        assert_eq!(from_load.names, vec!["base.esm", "second.esm"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// S0-3: a master list is a load order, and a master that another requires is
/// ordered first — for every game, because the rule is the engine's, not one
/// game's.
#[test]
fn every_game_orders_masters_by_their_dependencies() {
    for game in nvre_editor::game::ALL {
        let root = std::env::temp_dir().join(format!("nvre-masters-{}", game.id));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // The base master, and a DLC that requires it.
        nvre_edit::Document::create_empty("Base.esm", true, &[], None, None)
            .unwrap()
            .save(root.join("Base.esm"))
            .unwrap();
        nvre_edit::Document::create_empty("Dlc.esm", true, &["Base.esm".to_string()], None, None)
            .unwrap()
            .save(root.join("Dlc.esm"))
            .unwrap();

        let present = game.masters_in(&root);
        assert_eq!(
            present,
            vec!["Base.esm", "Dlc.esm"],
            "{}: the DLC cannot load before the master it requires",
            game.id
        );

        // Reversed on purpose: the ordering must not depend on how the folder
        // happens to list them.
        let declared = vec!["Dlc.esm".to_string(), "Base.esm".to_string()];
        let ordered = nvre_editor::game::order_named(
            &declared,
            &root,
            &nvre_editor::plugin_order::Rank::default(),
        );
        assert_eq!(
            ordered,
            vec!["Base.esm", "Dlc.esm"],
            "{}: the declared masters, base first",
            game.id
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
