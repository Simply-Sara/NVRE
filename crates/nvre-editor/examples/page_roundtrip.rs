//! Does a page lose anything? Load, save, and compare the record.
//!
//! Every hand-written page has its own loader and its own saver: `load_from_session`
//! parses the record into the page's fields, and `save_to_session` writes a record
//! back out of those fields. A subrecord the loader never parsed and the saver never
//! writes is a subrecord the page silently drops — and because the editor's save
//! writes the page's fields, not the file's bytes, dropping one is data loss.
//!
//! That is the failure mode a page has that the schema grid cannot have. The grid
//! re-encodes the subrecords the record holds; a page rebuilds the record from its
//! own fields, so anything a page's author did not think of is gone.
//!
//! The check is exact: load a record, save the state into a fresh document, and
//! compare every subrecord's bytes with the original. A page that round-trips its
//! records loses nothing; a page that does not has a list of the subrecords it
//! drops, which is the work to do on it.
//!
//! Usage:
//!
//! ```text
//! page_roundtrip                 # every installed game, 3 records per type
//! page_roundtrip --game fnv      # one game
//! page_roundtrip --sig WEAP      # one record type, listing what it drops
//! page_roundtrip --sample 20     # a wider sample
//! ```

use nvre_editor::game::{GameProfile, ALL};
use nvre_editor::object_editor::ObjectEditorState;
use nvre_editor::session::Session;

fn main() {
    let mut games: Vec<&str> = Vec::new();
    let mut only_sig: Option<String> = None;
    let mut sample: usize = 3;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--game" => {
                if let Some(v) = args.next() {
                    games.push(Box::leak(v.into_boxed_str()));
                }
            }
            "--sig" => only_sig = args.next(),
            "--sample" => sample = args.next().and_then(|v| v.parse().ok()).unwrap_or(3),
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
        let Ok(mut session) = Session::open(
            &master,
            &game.schema_dirs().0,
            vec![game.master.to_string()],
        ) else {
            eprintln!("{}: cannot open", game.id);
            continue;
        };
        let t = run(game, &mut session, only_sig.as_deref(), sample);
        totals.push((game.id.to_string(), t));
    }

    println!();
    println!(
        "{:<9} {:>7} {:>7} {:>9} {:>8} {:>9}",
        "game", "pages", "records", "lossless", "lossy", "dropped"
    );
    for (id, t) in &totals {
        println!(
            "{:<9} {:>7} {:>7} {:>9} {:>8} {:>9}",
            id, t.pages, t.records, t.lossless, t.lossy, t.dropped
        );
    }
    println!();
    println!("pages   = hand-written pages measured (a described or grid page is not code)");
    println!("records = records loaded and saved through the page's own load/save");
    println!("lossless= records that came back byte-identical");
    println!("lossy   = records that did not");
    println!("dropped = subrecords across all of them that the page did not write back");
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
    pages: usize,
    records: usize,
    lossless: usize,
    lossy: usize,
    dropped: usize,
}

fn run(game: &GameProfile, session: &mut Session, only_sig: Option<&str>, sample: usize) -> Totals {
    let mut out = Totals::default();
    let Ok(lo) = session.load_order() else {
        return out;
    };
    let mut seen: std::collections::BTreeMap<[u8; 4], Vec<u32>> = std::collections::BTreeMap::new();
    for (pi, _) in lo.plugins.iter().enumerate() {
        for rec in lo.records[pi].iter().copied() {
            let fid = rec.header.form_id;
            let v = seen.entry(rec.header.sig).or_default();
            if v.len() < sample && !v.contains(&fid) {
                v.push(fid);
            }
        }
    }
    for (sig, fids) in seen {
        let key = String::from_utf8_lossy(&sig).into_owned();
        if let Some(want) = only_sig {
            if !key.eq_ignore_ascii_case(want) {
                continue;
            }
        }
        // Read what the table says and let the schema borrow end here: the save
        // below needs the session mutably, and a live `&session.schema` would
        // outlive it.
        let (described, editor): (bool, Option<String>) = session
            .schema
            .object_window
            .categories
            .get(&key)
            .map(|c| (!c.sections.is_empty(), c.editor.clone()))
            .unwrap_or((false, None));
        let Some(name) = editor else { continue };
        // A page wins over sections in the object editor, so a category that names
        // one is measured here; everything else is a described page and writes
        // through the schema, which `editor_matrix` measures instead. Filtering on
        // "has sections" would now skip every named page, since every category has
        // sections.
        if name == "grid" {
            continue;
        }
        let _ = described;
        if nvre_editor::object_editor::editor_block(&name).is_none() {
            continue;
        }
        out.pages += 1;
        let mut page_lossy = false;
        for fid in fids {
            let Some(original) = session.resolved_record(fid) else {
                continue;
            };
            let Ok(orig_subs) = original.subrecords() else {
                continue;
            };
            let mut state = ObjectEditorState::default();
            state.load_from_session(session, fid, sig);
            // The page's own save, into **this** session.
            //
            // A second `Session::open` per record is what this used to do, and it
            // is not affordable: opening a session parses the whole master and
            // keeps its plugins for the process's life, so every record leaked
            // another master's worth of memory and the probe was killed by the
            // kernel before it finished a single game.
            //
            // Saving in place is sound because `save_to_session` writes only the
            // record the state was loaded from — it addresses `self.form_id` and
            // nothing else. The original bytes are read out *before* the save, so
            // the comparison is against the file's copy, not a mutated one.
            let before: Vec<([u8; 4], Vec<u8>)> =
                orig_subs.iter().map(|s| (s.sig, s.data.clone())).collect();
            state.save_to_session(session);
            let Some(saved) = session.resolved_record(fid) else {
                continue;
            };
            out.records += 1;
            let Ok(saved_subs) = saved.subrecords() else {
                continue;
            };
            // Every subrecord the original had must come back, **by occurrence**.
            //
            // A signature can repeat — an `NPC_` carries a dozen `CNTO`
            // subrecords, a weather a dozen texture layers — so comparing by
            // signature alone would line each one up against the first of its
            // kind and report every later one as changed. The nth `CNTO` has to
            // be compared with the nth `CNTO`.
            let mut seen: std::collections::BTreeMap<[u8; 4], usize> =
                std::collections::BTreeMap::new();
            let mut lost: Vec<String> = Vec::new();
            for (sub_sig, sub_data) in &before {
                let n = seen.entry(*sub_sig).or_insert(0);
                let after = saved_subs.iter().filter(|s| s.sig == *sub_sig).nth(*n);
                *n += 1;
                let tag = nvre_util::tag_str(sub_sig);
                let nth = if *n > 1 { format!("{tag}[{n}]") } else { tag };
                match after {
                    Some(a) if a.data == *sub_data => {}
                    Some(_) => lost.push(format!("{nth}(changed)")),
                    None => lost.push(format!("{nth}(dropped)")),
                }
            }
            if lost.is_empty() {
                out.lossless += 1;
            } else {
                out.lossy += 1;
                page_lossy = true;
                out.dropped += lost.len();
                if only_sig.is_some() {
                    eprintln!("  {:08X} {key}: {}", fid, lost.join(" "));
                }
            }
        }
        if page_lossy && only_sig.is_some() {
            eprintln!("  {key}: page {name:?} is lossy");
        }
        let _ = game;
    }
    out
}
