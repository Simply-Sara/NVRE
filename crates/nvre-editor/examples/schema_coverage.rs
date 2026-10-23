//! Report every record signature in each game's master against the schema's
//! object-window table, so the table's coverage can be checked without
//! opening the editor.
//!
//! For each installed game this prints one JSON object per signature:
//!
//! ```text
//! {"game":"fnv","sig":"WEAP","objects":1234,"in_table":true,"hidden":false,"record":"Weapon"}
//! ```
//!
//! `objects` counts the records of that signature the palette would walk
//! (unique FormIDs across the load order, as the Objects window sees them),
//! `in_table` says whether `object_window.categories` names the signature,
//! `hidden` whether that entry is marked `hidden`, and `record` is the
//! schema record definition's name, if any. A signature with objects but no
//! visible table entry is one the window does not list.
//!
//! The schema-coverage half needs no game data and always runs: it prints
//! signatures the FNV schema defines as records but its object-window table
//! does not mention, and table entries with no record definition to name
//! them.

use nvre_editor::game::GameProfile;

/// Every game the editor knows, in reporting order.
const GAMES: &[&str] = &["fnv", "fo3", "skyrim", "skyrimse", "fo4"];

fn main() {
    // Schema coverage first: it needs no installed game.
    for id in GAMES {
        let Some(game) = GameProfile::by_id(id) else {
            continue;
        };
        let Ok(schema) = game.load_schema() else {
            eprintln!("{id}: schema does not load");
            continue;
        };
        let mut missing: Vec<&str> = schema
            .records
            .keys()
            .filter(|s| !schema.object_window.categories.contains_key(*s))
            .map(String::as_str)
            .collect();
        missing.sort_unstable();
        let mut nameless: Vec<&str> = schema
            .object_window
            .categories
            .iter()
            .filter(|(s, c)| c.name.is_none() && !schema.records.contains_key(*s))
            .map(|(s, _)| s.as_str())
            .collect();
        nameless.sort_unstable();
        println!(
            "{id}: records without an object-window entry ({}): {}",
            missing.len(),
            missing.join(" ")
        );
        println!(
            "{id}: table entries with no record def and no name ({}): {}",
            nameless.len(),
            nameless.join(" ")
        );
    }

    // Per-game inventory, against each installed game's own master.
    for id in GAMES {
        let Some(game) = GameProfile::by_id(id) else {
            continue;
        };
        let Some(data) = game.data_dir() else {
            eprintln!("{id}: no data dir, skipped");
            continue;
        };
        let master = data.join(game.master);
        if !master.exists() {
            eprintln!("{id}: master not installed, skipped");
            continue;
        }
        let mut session = match nvre_editor::session::Session::open(
            &master,
            &game.schema_dirs().0,
            vec![game.master.to_string()],
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{id}: cannot open: {e}");
                continue;
            }
        };
        // Walk the load order exactly as the Objects window does: every
        // record, deduplicated by global FormID.
        let lo = session.load_order().expect("a load order");
        let mut seen: std::collections::HashMap<u32, [u8; 4]> = std::collections::HashMap::new();
        for (pi, _plugin) in lo.plugins.iter().enumerate() {
            for r in &lo.records[pi] {
                let Some(global) = lo.to_global(pi, r.header.form_id) else {
                    continue;
                };
                seen.entry(global).or_insert(r.header.sig);
            }
        }
        let mut counts: std::collections::BTreeMap<[u8; 4], usize> =
            std::collections::BTreeMap::new();
        for sig in seen.values() {
            *counts.entry(*sig).or_default() += 1;
        }
        let schema = &session.schema;
        for (sig, objects) in counts {
            let key = String::from_utf8_lossy(&sig).into_owned();
            let cat = schema.object_window.categories.get(&key);
            println!(
                "{{\"game\":\"{id}\",\"sig\":\"{key}\",\"objects\":{objects},\"in_table\":{},\"hidden\":{},\"record\":{}}}",
                cat.is_some(),
                cat.is_some_and(|c| c.hidden),
                match schema.records.get(&key) {
                    Some(r) => serde_json::to_string(&r.name).unwrap_or_default(),
                    None => "null".to_string(),
                }
            );
        }
        // The window's own view of the master: what `object_groups` — and
        // so the Objects window's table — actually lists, with each row's
        // Editor ID, FormID and name as the table shows them.
        let groups = session.object_groups("").to_vec();
        let total: usize = groups.iter().map(|g| g.objects.len()).sum();
        println!(
            "{id}: the palette lists {total} objects in {} categories",
            groups.len()
        );
        for g in groups.iter().take(2) {
            if let Some(row) = g.objects.first() {
                println!(
                    "{id}: {}'s first row: edid={:?} formid={:08X} name={:?}",
                    g.category, row.edid, row.fid, row.name
                );
            }
        }
    }
}
