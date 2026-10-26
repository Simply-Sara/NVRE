//! What does a real `LIGH` record's DATA decode to, per game?
//!
//! Usage: `ligh_probe <game id> <data dir>`
//!
//! Reads the first `LIGH` records in the game's master and prints their `DATA`
//! fields as the schema decodes them. The question is whether the layout this
//! repo reads is the layout the file has: `LIGH`'s DATA is documented for the
//! Fallout family, and Skyrim's is a separate layout that this schema marks as
//! a hypothesis with `DATA` as raw bytes.
use nvre_edit::encode;
use nvre_editor::game::GameProfile;
use nvre_esm::LoadOrder;
use serde_json::Value;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let game_id = args.next().expect("a game id");
    let data_dir = args.next().expect("a data folder");

    let profile = GameProfile::by_id(&game_id).expect("a known game");
    println!("game {}", profile.name);
    println!(
        "  schema: family={:?} version={:?}",
        profile.schema_family_dir, profile.schema_version_dir
    );
    // The **ESM** schema, not the NIF one: `encode::decoded` takes the record
    // schema, and `load_nif_schema` would hand it the block schema.
    let schema = profile
        .load_schema_over(None)
        .expect("the game's record schema");

    // The master plugin. It is a loose file in the Data folder, not in an
    // archive, so it is read from the path rather than from the VFS.
    let master_path = PathBuf::from(&data_dir).join(profile.master);
    let master = std::fs::read(&master_path).expect("the master is in the Data folder");
    let plugin =
        nvre_esm::Plugin::parse(&master, profile.master.to_string()).expect("the master parses");
    let lo = LoadOrder::new(std::slice::from_ref(&plugin)).expect("the load order");

    let mut found = 0usize;
    for rec in plugin
        .records()
        .into_iter()
        .filter(|r| r.header.sig == *b"LIGH")
    {
        println!("\n=== LIGH ===");
        // Only `DATA` is printed, in file order. An earlier version of this
        // probe printed every subrecord, and the order the parser hands them
        // back is not the order they are in the file — so `FNAM` was read as
        // `DATA` and the probe reported a layout that was not there.
        let subs = rec.subrecords().expect("the subrecords");
        let mut seen_data = false;
        for s in &subs {
            if s.sig.as_slice() != b"DATA" {
                continue;
            }
            seen_data = true;
            print!("  DATA    {:>6} bytes", s.data.len());
            match encode::decoded(&schema, b"LIGH", s) {
                Some(Value::Object(o)) => {
                    let mut keys: Vec<_> = o.keys().cloned().collect();
                    keys.sort();
                    println!("  decoded: {keys:?}");
                    for k in &keys {
                        println!("      {k} = {}", json_brief(&o[k]));
                    }
                }
                Some(other) => println!("  decoded (non-object): {other}"),
                None => println!("  NOT DECODED"),
            }
        }
        if !seen_data {
            println!("  (no DATA subrecord)");
        }
        found += 1;
        if found >= 3 {
            break;
        }
    }
    if found == 0 {
        println!("no LIGH record in the master");
    }
    let _ = lo;
}

/// A one-line rendering of a decoded JSON value.
fn json_brief(v: &Value) -> String {
    match v {
        Value::Array(a) => format!(
            "[{}]",
            a.iter()
                .take(6)
                .map(json_brief)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("{k}: {}", json_brief(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        other => other.to_string(),
    }
}
