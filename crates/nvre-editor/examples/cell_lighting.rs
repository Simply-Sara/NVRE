//! What a cell's `XCLL` lighting subrecord actually holds, per game.
//!
//! Usage: `cell_lighting <game id> <data dir>`
//!
//! A cell's own lighting is the `XCLL` subrecord: the ambient colour, the
//! directional colour and the fog the renderer draws with. This prints the
//! fields for a few interior cells of a master, so the reader is written
//! against the real byte layout rather than a guess.
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

    let cells = session.cells(CellScope::Interiors);
    let mut shown = 0;
    for c in &cells {
        let Some(bytes) = session.cell_sub(c.form_id, b"XCLL") else {
            continue;
        };
        if bytes.len() < 40 {
            println!(
                "{} XCLL is {} bytes — too short",
                c.editor_id(),
                bytes.len()
            );
            continue;
        }
        let f32_at =
            |o: usize| f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        let i32_at =
            |o: usize| i32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        println!(
            "{} ({} bytes)\n  ambient {},{},{}  directional {},{},{}  fog {},{},{}",
            c.editor_id(),
            bytes.len(),
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[4],
            bytes[5],
            bytes[6],
            bytes[8],
            bytes[9],
            bytes[10],
        );
        println!(
            "  fog_near {}  fog_far {}  rot_xy {}  rot_z {}  fade {}  clip {}  power {}",
            f32_at(12),
            f32_at(16),
            i32_at(20),
            i32_at(24),
            f32_at(28),
            f32_at(32),
            f32_at(36)
        );
        shown += 1;
        if shown >= 6 {
            break;
        }
    }
}
