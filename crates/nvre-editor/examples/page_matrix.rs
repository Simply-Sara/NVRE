//! What each kind of page delivers, per game.
//!
//! `editor_matrix` measures fields: every field of every record type, changed,
//! read back, changed back, bytes restored. That answers "does the schema and the
//! encoder agree", which is the grid's own contract — and it is the contract every
//! page is built on, because a page's widget ends in the same `set_path` and the
//! same subrecord re-encode.
//!
//! It does not answer "what does a modder get when they open this record type".
//! A page can cover every subrecord its record has, or half of them, and the
//! matrix's field counts would not say which. This does: for each record type, in
//! each game, the page the object editor opens and how much of the record that
//! page puts in front of a modder.
//!
//! Three page kinds, and the difference matters:
//!
//! - `page`: a hand-written page. It shows the fields it was written for, which
//!   is a subset of the layout by construction — Fallout: New Vegas's weapon page
//!   predates a dozen subrecords the schema now knows.
//! - `sections`: a page described by its schema sections. It shows every
//!   subrecord, grouped, because the data names them all.
//! - `grid`: no page, so every subrecord, flat.
//!
//! Usage:
//!
//! ```text
//! page_matrix                 # every installed game
//! page_matrix --game fo4      # one game
//! page_matrix --sig WEAP      # one record type, with the page's section list
//! ```

use nvre_editor::game::{GameProfile, ALL};
use nvre_editor::object_editor::Pages;
use nvre_editor::session::Session;

fn main() {
    let mut games: Vec<&str> = Vec::new();
    let mut only_sig: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--game" => {
                if let Some(v) = args.next() {
                    games.push(Box::leak(v.into_boxed_str()));
                }
            }
            "--sig" => only_sig = args.next(),
            other => {
                eprintln!("unknown argument {other:?}");
                std::process::exit(2);
            }
        }
    }
    let selected: Vec<&'static GameProfile> = if games.is_empty() {
        ALL.iter().collect()
    } else {
        games
            .iter()
            .filter_map(|id| GameProfile::by_id(id))
            .collect()
    };

    let mut totals: Vec<(String, Totals)> = Vec::new();
    for game in selected {
        let Some(data) = data_dir_for(game) else {
            eprintln!("{}: not installed, skipped", game.id);
            continue;
        };
        let master = data.join(game.master);
        if !master.exists() {
            eprintln!("{}: {} not installed, skipped", game.id, game.master);
            continue;
        }
        let Ok(session) = Session::open(
            &master,
            &game.schema_dirs().0,
            vec![game.master.to_string()],
        ) else {
            eprintln!("{}: cannot open", game.id);
            continue;
        };
        let t = run(game, &session, only_sig.as_deref());
        totals.push((game.id.to_string(), t));
    }

    println!();
    println!(
        "{:<9} {:>5} {:>7} {:>8} {:>6} {:>8} {:>10} {:>11}",
        "game", "sigs", "written", "described", "grid", "subs", "measurable", "unmeasurable"
    );
    for (id, t) in &totals {
        println!(
            "{:<9} {:>5} {:>7} {:>8} {:>6} {:>8} {:>9.1}% {:>11}",
            id,
            t.sigs,
            t.written,
            t.described,
            t.grid,
            t.subs,
            if t.subs == 0 {
                0.0
            } else {
                t.covered as f64 / t.subs as f64 * 100.0
            },
            t.unmeasured
        );
    }
    println!();
    println!("written   = a hand-written page: it draws the fields it was written for,");
    println!("            which is a subset of the layout and is not countable without");
    println!("            running the UI, so its subrecords are the unmeasurable column");
    println!("described = a page described by its schema sections: it shows every");
    println!("            subrecord the record carries (tests/sections.rs asserts it)");
    println!("grid      = no page: every subrecord, flat");
    println!("measurable= share of subrecords whose presence on the page is a fact");
}

fn data_dir_for(game: &GameProfile) -> Option<std::path::PathBuf> {
    if let Some(dir) = game.data_dir() {
        return Some(dir);
    }
    let known = format!("/home/sara/Games/BethesdaGames/{}/Data", game.name);
    let p = std::path::PathBuf::from(known);
    p.join(game.master).exists().then_some(p)
}

#[derive(Default)]
struct Totals {
    sigs: usize,
    written: usize,
    described: usize,
    grid: usize,
    subs: usize,
    /// Subrecords on a page whose coverage is a fact about the schema.
    covered: usize,
    /// Subrecords on a page whose coverage is a fact about the page's code.
    unmeasured: usize,
}

fn run(game: &GameProfile, session: &Session, only_sig: Option<&str>) -> Totals {
    let mut out = Totals::default();
    let schema = &session.schema;
    let mut pages = Pages::default();
    let Ok(lo) = session.load_order() else {
        return out;
    };
    // One record per signature is enough: the page a type gets is a property of
    // the type, not of the record.
    let mut seen: std::collections::BTreeMap<[u8; 4], &nvre_esm::Record> =
        std::collections::BTreeMap::new();
    for (pi, _) in lo.plugins.iter().enumerate() {
        for rec in lo.records[pi].iter().copied() {
            seen.entry(rec.header.sig).or_insert(rec);
        }
    }
    for (sig, rec) in seen {
        let key = String::from_utf8_lossy(&sig).into_owned();
        if let Some(want) = only_sig {
            if !key.eq_ignore_ascii_case(want) {
                continue;
            }
        }
        let Ok(subs) = rec.subrecords() else { continue };
        let n_subs = subs.len();
        let cat = schema.object_window.categories.get(&key);
        // The flattened page: a template's sections plus this game's own. Asking
        // the category directly would miss everything a page inherits.
        let described = schema.page_is_described(&key);
        let name = cat.and_then(|c| c.editor.as_deref());
        // `Some(n)` when the number of subrecords a page shows is a property of
        // the schema; `None` when it is a property of the page's code.
        let (kind, page_serves, cover): (&str, bool, Option<usize>) = if described {
            // A sectioned page shows the subrecords its sections name, and the
            // renderer shows the rest under "Everything else" — so it shows
            // every subrecord, which the sections test asserts is all of them.
            let named = schema
                .page_sections(&key)
                .iter()
                .map(|s| s.subs.len())
                .sum::<usize>();
            ("sections", true, Some(named.min(n_subs)))
        } else if let Some(name) = name {
            match pages.get(name) {
                Some(page) => {
                    let serves = page.games().contains(&game.id);
                    // A hand-written page shows the fields it was written for,
                    // which is not a number the schema holds. What can be said
                    // without opening the window is that it covers the record
                    // only if it was written for it; the rest is measured by
                    // the field matrix, which is why that exists.
                    // A hand-written page draws what its code draws. Counting
                    // that needs the window, so it is reported as unmeasurable
                    // rather than guessed at as zero.
                    (if serves { "written" } else { "wrong game" }, serves, None)
                }
                None => ("typo", false, None),
            }
        } else {
            ("grid", true, Some(n_subs))
        };
        out.sigs += 1;
        out.subs += n_subs;
        match kind {
            "written" => out.written += 1,
            "sections" => out.described += 1,
            "grid" => out.grid += 1,
            _ => {}
        }
        match cover {
            Some(n) => out.covered += n,
            None => out.unmeasured += n_subs,
        }
        println!(
            "{{\"game\":\"{}\",\"sig\":\"{}\",\"page\":\"{}\",\"serves\":{},\"subs\":{},\
             \"covered\":{}}}",
            game.id,
            key,
            kind,
            page_serves,
            n_subs,
            match cover {
                Some(n) => n.to_string(),
                None => "null".to_string(),
            }
        );
        if only_sig.is_some() && described {
            for section in schema.page_sections(&key) {
                eprintln!("  {}: {}", section.title, section.subs.join(" "));
            }
        }
    }
    out
}
