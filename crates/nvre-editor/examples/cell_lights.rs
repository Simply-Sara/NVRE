//! Which cells in a master hold `LIGH` references, and how many.
//!
//! Usage: `cell_lights <game id> <data dir>`
//!
//! The question this answers: a light is only drawn for a cell the editor can
//! open, so a cell with lamps is the thing to look for. It also reports whether
//! the light's base object resolves, which is the step before the spawn.
use nvre_editor::game::GameProfile;
use nvre_editor::session::{CellScope, Session};
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let game_id = args.next().expect("a game id");
    let data_dir = args.next().expect("a data folder");
    let profile = GameProfile::by_id(&game_id).expect("a known game");
    println!("game {}", profile.name);

    let master = PathBuf::from(&data_dir).join(profile.master);
    let schema_dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas"))
        .join(profile.schema_subdir);
    let mut session = Session::open(
        master.as_path(),
        schema_dir.as_path(),
        vec![profile.master.to_string()],
    )
    .expect("the master opens");

    let mut rows: Vec<(String, usize, usize)> = Vec::new();
    for w in session.worldspaces() {
        for c in session.cells(CellScope::Worldspace(w.form_id)) {
            session.open_cell(c.form_id);
            let refs = session.cell_refs().to_vec();
            let mut lights = 0usize;
            let mut resolved = 0usize;
            for r in &refs {
                let Some(base) = r
                    .base
                    .as_deref()
                    .and_then(|b| u32::from_str_radix(b.strip_prefix("0x")?, 16).ok())
                else {
                    continue;
                };
                if session.light_for(base).is_some() {
                    lights += 1;
                    resolved += 1;
                }
            }
            if lights > 0 {
                rows.push((
                    c.name.clone().unwrap_or(c.edid.clone().unwrap_or_default()),
                    lights,
                    resolved,
                ));
            }
        }
    }
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    println!("{} cells hold lights; the top ten:", rows.len());
    for (name, lights, resolved) in rows.iter().take(10) {
        println!("  {name:<44} {lights} lights, {resolved} resolved");
    }
}
