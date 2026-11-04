//! Every record type a game's schema knows, and the subrecords it holds.
//!
//! One JSON line per record type:
//!
//! ```text
//! {"sig":"WEAP","subs":["APPR","BAMT",...]}
//! ```
//!
//! This exists so tools that describe a page (`tools/pages_from_schema.py`) work
//! from the **loaded** schema rather than re-reading the JSON files. The chain is
//! `base -> family -> version` and a later layer replaces a record whole unless it
//! says `merge`; a tool that re-implemented that would be a second copy of the rule
//! and would drift from it. Reading it from `Schema` means there is one answer.
//!
//! Usage:
//!
//! ```text
//! dump_subs --game fnv            # one line per record type
//! dump_subs --game fo4 --sig WEAP # one type
//! ```

use nvre_editor::game::{GameProfile, ALL};

fn main() {
    let mut game = String::from("fnv");
    let mut only_sig: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--game" => game = args.next().unwrap_or(game),
            "--sig" => only_sig = args.next(),
            "--all" => {}
            other => {
                eprintln!("unknown argument {other:?}");
                std::process::exit(2);
            }
        }
    }
    let Some(profile) = GameProfile::by_id(&game) else {
        eprintln!("{game}: no such game");
        std::process::exit(2);
    };
    let schema = match profile.load_schema() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{game}: {e}");
            std::process::exit(1);
        }
    };
    for (sig, rec) in &schema.records {
        if let Some(want) = &only_sig {
            if !sig.eq_ignore_ascii_case(want) {
                continue;
            }
        }
        let subs: Vec<&str> = rec.subrecords.keys().map(String::as_str).collect();
        println!(
            "{}",
            serde_json::json!({"sig": sig, "subs": subs})
        );
    }
    let _ = ALL;
}
