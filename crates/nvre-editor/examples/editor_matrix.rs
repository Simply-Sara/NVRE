//! Does the object editor work in this game?
//!
//! The object editor is one window over every game: a record type gets a
//! bespoke page when the schema's object-window table names one, and the schema
//! grid otherwise. Either way what a modder edits is a *field*, and the editor
//! writes it by decoding the whole subrecord, replacing the value, and
//! re-encoding — so "the editor works" has a precise meaning that can be
//! measured rather than looked at:
//!
//! 1. every subrecord of every record type decodes with the layout the game
//!    used (the record's block sequence decides which one that is);
//! 2. a subrecord the editor shows as editable re-encodes to its own bytes;
//! 3. a field the grid shows a widget for can be **changed** and read back as
//!    the new value, and **changed back** to its original value and re-encode to
//!    the original bytes;
//! 4. a field the schema declares and the grid would list is actually reachable
//!    in the decoded value.
//!
//! The third is the one a decode-only check misses: an encoder that writes a
//! field but a decoder that does not read it back (or the reverse) round-trips
//! identity perfectly and still loses the edit. The fourth is the one a
//! round-trip check misses: a row whose path is not in the decoded value is
//! silently skipped by the grid, so the field is not editable and nothing says
//! so.
//!
//! This walks each installed game's own master and prints one JSON object per
//! record type, then a summary per game. Nothing here is game-specific: it asks
//! the schema the same questions the editor's grid asks it, through the same
//! functions (`sub_views`, `field_names_in_block`, `set_path`,
//! `encode_sub_len_in_block`), so a green line here is the grid's own path being
//! green and not a second implementation agreeing with itself.
//!
//! Usage:
//!
//! ```text
//! editor_matrix                 # every installed game, 4 records per type
//! editor_matrix --limit 12      # a wider sample
//! editor_matrix --game fo4      # one game
//! editor_matrix --sig WEAP      # one record type, with per-field detail
//! ```

use nvre_editor::game::{GameProfile, ALL};
use nvre_editor::session::Session;
use serde_json::Value;

/// How many records of one type to sample.
const DEFAULT_LIMIT: usize = 4;

fn main() {
    let mut limit = DEFAULT_LIMIT;
    let mut games: Vec<&str> = Vec::new();
    let mut only_sig: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--limit" => {
                limit = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(DEFAULT_LIMIT)
            }
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
            .filter_map(|id| match GameProfile::by_id(id) {
                Some(g) => Some(g),
                None => {
                    eprintln!("{id}: no such game");
                    None
                }
            })
            .collect()
    };

    let mut totals: Vec<(String, GameTotals)> = Vec::new();
    for game in selected {
        let Some(data) = data_dir_for(game) else {
            eprintln!("{}: not installed, skipped", game.id);
            continue;
        };
        let master = data.join(game.master);
        if !master.exists() {
            eprintln!(
                "{}: {} not in {}, skipped",
                game.id,
                game.master,
                data.display()
            );
            continue;
        }
        eprintln!(
            "{}: opening {} (schema {})",
            game.id,
            master.display(),
            game.schema_dirs().0.display()
        );
        let session = match Session::open(
            &master,
            &game.schema_dirs().0,
            vec![game.master.to_string()],
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: cannot open: {e}", game.id);
                continue;
            }
        };
        let t = run_game(game, &session, limit, only_sig.as_deref());
        totals.push((game.id.to_string(), t));
    }

    println!();
    println!(
        "{:<9} {:>5} {:>6} {:>6} {:>5} {:>5} {:>8} {:>8} {:>8} {:>7} {:>8} {:>5} {:>6} {:>6} {:>5}",
        "game",
        "sigs",
        "paged",
        "sects",
        "grid",
        "typo",
        "fields",
        "reach",
        "editable",
        "ok",
        "skipped",
        "bound",
        "counted",
        "miss",
        "fail"
    );
    for (id, t) in &totals {
        println!(
            "{:<9} {:>5} {:>6} {:>6} {:>5} {:>5} {:>8} {:>8} {:>8} {:>7} {:>8} {:>5} {:>6} {:>6} {:>5}",
            id,
            t.sigs,
            t.paged,
            t.sectioned,
            t.grid,
            t.broken,
            t.fields,
            t.reachable,
            t.editable,
            t.ok,
            t.skipped,
            t.bounds,
            t.derived,
            t.missing,
            t.failed
        );
    }
    println!();
    println!("paged = a bespoke page the schema names and this build registers");
    println!("sects = a page described by the schema's sections (data, not code)");
    println!("grid  = no page: the schema grid is what the object editor shows");
    println!("typo  = the schema names a page this build does not register");
    println!("reach = fields whose path is in the decoded value (the grid drops the rest)");
    println!("ok    = changed, read back, changed back, bytes restored");
    println!("skip  = strings and flag words: identity only, no change attempted");
    println!("bound = the changed value did not fit its type (the sample hit 255/0)");
    println!("counted = a derived count: shown, not editable (the encoder derives it)");
    println!("miss  = declared but not in the decoded value: array/union/optional");
    println!("fail  = a real disagreement between the grid, the decoder and the encoder");
}

/// Where the game's Data folder is, for a machine that keeps some of them
/// outside Steam's own libraries.
///
/// `GameProfile::data_dir` is the editor's own answer — the user's saved
/// override, else the Steam install — and it is tried first, so the matrix sees
/// the same folder the editor would. `NVRE_DATA_<GAME>` is the escape hatch for
/// a copy Steam does not know about, which is how the corpora on this machine
/// are laid out.
fn data_dir_for(game: &GameProfile) -> Option<std::path::PathBuf> {
    if let Some(dir) = game.data_dir() {
        return Some(dir);
    }
    let key = format!("NVRE_DATA_{}", game.id.to_ascii_uppercase());
    if let Ok(v) = std::env::var(&key) {
        let p = std::path::PathBuf::from(v);
        if p.join(game.master).exists() {
            return Some(p);
        }
    }
    let known = format!("/home/sara/Games/BethesdaGames/{}/Data", game.name);
    let p = std::path::PathBuf::from(known);
    p.join(game.master).exists().then_some(p)
}

#[derive(Default)]
struct GameTotals {
    /// Record types with at least one record in the master.
    sigs: usize,
    /// Types with a bespoke page the schema names and this build registers.
    paged: usize,
    /// Types the object editor falls back to the schema grid for.
    grid: usize,
    /// Types whose page is described by the schema's sections rather than code.
    sectioned: usize,
    /// Types whose schema names a page this build does not register.
    broken: usize,
    fields: usize,
    reachable: usize,
    editable: usize,
    ok: usize,
    skipped: usize,
    bounds: usize,
    derived: usize,
    missing: usize,
    failed: usize,
}

/// What one record type's sample came to.
#[derive(Default)]
struct SigTotals {
    records: usize,
    subs: usize,
    subs_decoded: usize,
    subs_writable: usize,
    fields: usize,
    reachable: usize,
    editable: usize,
    ok: usize,
    skipped: usize,
    bounds: usize,
    derived: usize,
    missing: usize,
    failed: usize,
    /// The first subrecord the editor will show read-only, and why.
    first_read_only: Option<String>,
    /// Why a field was not reachable, counted by kind.
    missing_why: std::collections::BTreeMap<&'static str, usize>,
    /// The first failure, with the field path and the reason, so the line says
    /// what to fix rather than only that something is wrong.
    first_failure: Option<String>,
}

/// The outcome of checking one field.
enum Check {
    /// Changed, read back, changed back, bytes restored.
    Ok,
    /// A string or a flag word: identity checked only.
    Skipped,
    /// The changed value did not fit the type (255 + 1 in a `u8`).
    Bound,
    /// A real disagreement: the value does not read back, or the bytes are not
    /// restored.
    Fail(String),
}

fn run_game(
    game: &GameProfile,
    session: &Session,
    limit: usize,
    only_sig: Option<&str>,
) -> GameTotals {
    let mut out = GameTotals::default();
    let schema = &session.schema;
    let Ok(lo) = session.load_order() else {
        eprintln!("{}: no load order", game.id);
        return out;
    };
    // Every record type in the master, in signature order, with the records to
    // sample. Deduplicated by signature only: the sample is about layouts, not
    // about how many of a type a plugin holds.
    let mut by_sig: std::collections::BTreeMap<[u8; 4], Vec<&nvre_esm::Record>> =
        std::collections::BTreeMap::new();
    for (pi, _) in lo.plugins.iter().enumerate() {
        for rec in lo.records[pi].iter().copied() {
            let v = by_sig.entry(rec.header.sig).or_default();
            if v.len() < limit {
                v.push(rec);
            }
        }
    }

    for (sig, records) in by_sig {
        let key = String::from_utf8_lossy(&sig).into_owned();
        if let Some(want) = only_sig {
            if !key.eq_ignore_ascii_case(want) {
                continue;
            }
        }
        let mut t = SigTotals::default();
        for rec in &records {
            t.records += 1;
            // How this record's text is stored, which decides whether a text
            // subrecord decodes as characters or as a string-table ID.
            let localized = session.record_is_localized(rec.header.form_id);
            let Ok(subs) = rec.subrecords() else { continue };
            let Ok(views) = nvre_edit::encode::sub_views(schema, rec, localized) else {
                continue;
            };
            for (sub, view) in subs.iter().zip(views.iter()) {
                t.subs += 1;
                if view.problem.is_none() {
                    t.subs_decoded += 1;
                }
                if view.writable {
                    t.subs_writable += 1;
                } else if t.first_read_only.is_none() {
                    // The first subrecord the editor will show read-only, so a
                    // signature with a gap names the record and the layout that
                    // caused it rather than only the count.
                    t.first_read_only = Some(format!(
                        "{:08X} {}{}: {}",
                        rec.header.form_id,
                        nvre_util::tag_str(&sub.sig),
                        match &view.block {
                            Some(b) => format!(" (in {b})"),
                            None => String::new(),
                        },
                        view.problem
                            .clone()
                            .unwrap_or_else(|| "does not re-encode".into()),
                    ));
                }
                let rows = nvre_edit::encode::rows(
                    schema,
                    &sig,
                    &sub.sig,
                    view.block.as_deref(),
                    &view.value,
                );
                if only_sig.is_some() && t.records == 1 && t.subs <= 12 {
                    eprintln!(
                        "  sub {} block={:?} derived={:?} rows={:?}",
                        nvre_util::tag_str(&sub.sig),
                        view.block,
                        nvre_edit::encode::derived_paths(
                            schema,
                            &sig,
                            &sub.sig,
                            view.block.as_deref()
                        ),
                        rows.iter()
                            .map(|r| format!("{}{}", r.path, if r.derived { "*" } else { "" }))
                            .collect::<Vec<_>>()
                    );
                }
                for row in &rows {
                    let path = &row.path;
                    t.fields += 1;
                    // The grid's own row lookup: a path that is not in the
                    // decoded value has no row at all. Value-derived rows make
                    // this impossible in principle, so a non-zero count here is
                    // a path the grid cannot address.
                    let Some(current) = get_path(&view.value, path) else {
                        let why = why_missing(&view.value, path);
                        *t.missing_why.entry(why).or_default() += 1;
                        t.missing += 1;
                        if t.first_failure.is_none() {
                            t.first_failure = Some(format!(
                                "{:08X} {}.{}: not in the decoded value ({why})",
                                rec.header.form_id,
                                nvre_util::tag_str(&sub.sig),
                                row_name(path),
                            ));
                        }
                        continue;
                    };
                    t.reachable += 1;
                    // A derived count is shown but not editable: the encoder
                    // derives it from the array it counts.
                    if row.derived {
                        t.derived += 1;
                        continue;
                    }
                    if !view.writable {
                        continue;
                    }
                    t.editable += 1;
                    match check_field(schema, &sig, sub, view, path, current, localized) {
                        Check::Ok => t.ok += 1,
                        Check::Skipped => t.skipped += 1,
                        Check::Bound => t.bounds += 1,
                        Check::Fail(why) => {
                            t.failed += 1;
                            if t.first_failure.is_none() {
                                t.first_failure = Some(format!(
                                    "{:08X} {}.{}: {why}",
                                    rec.header.form_id,
                                    nvre_util::tag_str(&sub.sig),
                                    row_name(path),
                                ));
                            }
                        }
                    }
                }
            }
        }
        // Which page the object editor would show for this type. The rule is the
        // object editor's own: a category with sections is a described page, and
        // otherwise the `editor` name decides, and a name this build does not
        // register is a typo.
        let entry = schema.object_window.categories.get(&key);
        let described = entry.is_some_and(|c| !c.sections.is_empty());
        let page = entry.and_then(|c| c.editor.as_deref());
        let page_state = if described {
            "sections"
        } else {
            match page {
                Some("grid" | "sections") => "page",
                Some(_) => "typo",
                None => "grid",
            }
        };
        match page_state {
            "page" => out.paged += 1,
            "sections" => out.sectioned += 1,
            "grid" => out.grid += 1,
            _ => out.broken += 1,
        }
        out.sigs += 1;
        out.fields += t.fields;
        out.reachable += t.reachable;
        out.editable += t.editable;
        out.ok += t.ok;
        out.skipped += t.skipped;
        out.bounds += t.bounds;
        out.derived += t.derived;
        out.missing += t.missing;
        out.failed += t.failed;

        println!(
            "{{\"game\":\"{}\",\"sig\":\"{}\",\"page\":\"{}\",\"records\":{},\"subs\":{},\
             \"subs_decoded\":{},\"subs_writable\":{},\"fields\":{},\"reachable\":{},\
             \"editable\":{},\"ok\":{},\"skipped\":{},\"bound\":{},\"derived\":{},\"missing\":{},\
             \"missing_why\":{},\"failed\":{},\"first_read_only\":{},\"first_failure\":{}}}",
            game.id,
            key,
            page_state,
            t.records,
            t.subs,
            t.subs_decoded,
            t.subs_writable,
            t.fields,
            t.reachable,
            t.editable,
            t.ok,
            t.skipped,
            t.bounds,
            t.derived,
            t.missing,
            serde_json::to_string(&t.missing_why).unwrap_or_else(|_| "{}".into()),
            t.failed,
            match &t.first_read_only {
                Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".into()),
                None => "null".to_string(),
            },
            match &t.first_failure {
                Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".into()),
                None => "null".to_string(),
            }
        );
        if only_sig.is_some() {
            eprintln!(
                "{} {}: {} records, {} subs ({} decoded, {} writable), {} fields \
                 ({} reachable, {} editable), {} ok, {} skipped, {} bound, {} derived, {} missing, \
                 {} failed",
                game.id,
                key,
                t.records,
                t.subs,
                t.subs_decoded,
                t.subs_writable,
                t.fields,
                t.reachable,
                t.editable,
                t.ok,
                t.skipped,
                t.bounds,
                t.derived,
                t.missing,
                t.failed
            );
            if let Some(ro) = &t.first_read_only {
                eprintln!("  first read-only: {ro}");
            }
        }
    }
    out
}

/// Change one field, read it back, change it back, and check the bytes.
///
/// This is the editor's write path with the widget left out: `set_path` is the
/// function the grid's widget calls, and `encode_sub_len_in_block` is what the
/// session calls to turn the decoded value into the subrecord the document
/// stores. The expected length is the original's, so a field whose layout
/// implies a different size fails here rather than resizing the record.
///
/// Only values that can be changed without inventing one are perturbed: a
/// number by one, a boolean by flipping it. A string is not appended to (a
/// fixed-size `char` array would overflow, and a zstring's encoding is not the
/// grid's business) and a flags object is not rebuilt (its `set` list and its
/// number have to agree, which is the widget's job, not this check's) — those
/// are counted as skipped and checked for identity only.
fn check_field(
    schema: &nvre_esm::Schema,
    rec_sig: &[u8; 4],
    sub: &nvre_esm::Subrecord,
    view: &nvre_edit::encode::SubView,
    path: &str,
    current: &Value,
    localized: bool,
) -> Check {
    // The values worth trying, in order. A number at the top of its range fails
    // to encode, so the other direction is tried before calling it a bound.
    let mut candidates: Vec<Value> = Vec::new();
    match current {
        Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                candidates.push(Value::from(u.saturating_add(1)));
                if u > 0 {
                    candidates.push(Value::from(u - 1));
                }
            } else if let Some(i) = n.as_i64() {
                candidates.push(Value::from(i.saturating_add(1)));
                candidates.push(Value::from(i.saturating_sub(1)));
            } else if let Some(f) = n.as_f64() {
                candidates.push(Value::from(f + 1.0));
                candidates.push(Value::from(f - 1.0));
            }
        }
        Value::Bool(b) => candidates.push(Value::from(!b)),
        // Strings, flags objects, arrays and null: identity only.
        _ => return identity(schema, rec_sig, sub, view),
    }

    let mut encoded: Option<(Value, Vec<u8>)> = None;
    let mut out_of_range = false;
    for candidate in candidates {
        let mut edited = view.value.clone();
        if nvre_editor::session::set_path(&mut edited, path, candidate.clone()).is_err() {
            return Check::Fail("the field cannot be replaced in the decoded value".into());
        }
        match nvre_edit::encode::encode_sub_len_in_block(
            schema,
            rec_sig,
            &sub.sig,
            &edited,
            Some(sub.data.len()),
            view.block.as_deref(),
        ) {
            Ok(bytes) => {
                encoded = Some((candidate, bytes));
                break;
            }
            Err(e) => {
                // A value that does not fit its type is the *sample's* fault, not
                // the layout's: a `u8` field at 255 cannot take 256. That is
                // reported as `bound`, and told apart from a real failure by the
                // error's own kind rather than by searching its text.
                if e.is_range() {
                    out_of_range = true;
                } else {
                    return Check::Fail(format!("the changed value does not encode: {e}"));
                }
            }
        }
    }
    let Some((changed, bytes)) = encoded else {
        return if out_of_range {
            Check::Bound
        } else {
            Check::Fail("the changed value does not encode".into())
        };
    };

    // Read it back through the same decoder the grid reads with.
    let reread = nvre_edit::encode::decoded_in_block_localized(
        schema,
        rec_sig,
        &nvre_esm::Subrecord {
            sig: sub.sig,
            data: bytes,
            extended: sub.extended,
        },
        view.block.as_deref(),
        localized,
    );
    let Some(reread) = reread else {
        return Check::Fail("the edited subrecord does not decode".into());
    };
    let Some(back) = get_path(&reread, path) else {
        return Check::Fail("the edited field does not decode".into());
    };
    if !same_value(back, &changed) {
        return Check::Fail(format!(
            "the edit does not read back: wrote {changed}, read {back}"
        ));
    }

    // Change it back, and the bytes must be the ones the file had.
    let mut restored = reread;
    if let Err(e) = nvre_editor::session::set_path(&mut restored, path, current.clone()) {
        return Check::Fail(format!("the field cannot be restored: {e}"));
    }
    match nvre_edit::encode::encode_sub_len_in_block(
        schema,
        rec_sig,
        &sub.sig,
        &restored,
        Some(sub.data.len()),
        view.block.as_deref(),
    ) {
        Ok(bytes) => {
            if bytes != sub.data {
                return Check::Fail(format!(
                    "changing the field back does not restore the bytes ({} vs {})",
                    bytes.len(),
                    sub.data.len()
                ));
            }
        }
        Err(e) => return Check::Fail(format!("the restored value does not encode: {e}")),
    }
    Check::Ok
}

/// Whether a value read back is the one that was written.
///
/// An `f32` field cannot read back as the `f64` the grid's `DragValue` handed
/// it — `1.2` is not representable, so it encodes as `1.2000000476837158` and
/// decodes as that. Comparing the two as `f64` would call every float field
/// broken; comparing them as `f32` is exactly the width the layout wrote.
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (Some(xf), Some(yf)) = (x.as_f64(), y.as_f64()) else {
                return x == y;
            };
            xf == yf || (xf as f32) == (yf as f32)
        }
        _ => a == b,
    }
}

/// Decode and re-encode the subrecord unchanged.
fn identity(
    schema: &nvre_esm::Schema,
    rec_sig: &[u8; 4],
    sub: &nvre_esm::Subrecord,
    view: &nvre_edit::encode::SubView,
) -> Check {
    match nvre_edit::encode::encode_sub_len_in_block(
        schema,
        rec_sig,
        &sub.sig,
        &view.value,
        Some(sub.data.len()),
        view.block.as_deref(),
    ) {
        Ok(bytes) if bytes == sub.data => Check::Skipped,
        Ok(_) => Check::Fail("re-encoding the unchanged value does not restore the bytes".into()),
        Err(e) => Check::Fail(format!("does not encode: {e}")),
    }
}

/// The value at a dotted path, the same walk the grid does.
fn get_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    nvre_edit::encode::value_at(value, path)
}

/// Why a path the schema declares is not in the decoded value.
///
/// - `array`: the parent is a list, and the grid has no editor for an element —
///   `SCOL`'s `DATA` is an array of placements, so its `position` is real and
///   unreachable.
/// - `absent`: an optional field this record does not carry, or another variant
///   of a union the record did not choose.
/// - `shape`: the decoded value is a scalar where the layout declares a struct,
///   which is a decoder/encoder disagreement rather than a missing field.
fn why_missing(value: &Value, path: &str) -> &'static str {
    if path.is_empty() {
        return "shape";
    }
    let mut at = value;
    for part in path.split('.') {
        match at {
            Value::Array(_) => return "array",
            Value::Object(o) => match o.get(part) {
                Some(v) => at = v,
                None => return "absent",
            },
            _ => return "shape",
        }
    }
    "absent"
}

/// A field path as the grid labels its row.
fn row_name(path: &str) -> &str {
    if path.is_empty() {
        "value"
    } else {
        path
    }
}
