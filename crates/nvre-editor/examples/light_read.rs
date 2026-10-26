//! Does `Session::light_for` read a real light? Per game.
//!
//! Usage: `light_read <game id> <data dir>`
//!
//! Opens the game's master as a session and prints the [`LightData`] the
//! viewport would be handed for its first `LIGH` records. This is the
//! end-to-end check: the schema layer, the record walk and the field reader
//! together, through the same call `draw_cell_lights` makes.
use nvre_editor::game::GameProfile;
use nvre_editor::session::Session;
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

    // The records are collected first: `light_for` needs `&mut self`, and the
    // borrow of the document the iterator holds would not allow it.
    let lighs: Vec<(u32, String)> = session
        .doc
        .records()
        .iter()
        .filter(|(_, r)| r.header.sig == *b"LIGH")
        .map(|(_, r)| {
            let edid = r
                .subrecords()
                .ok()
                .and_then(|s| {
                    s.iter()
                        .find(|x| x.sig.as_slice() == b"EDID")
                        .map(|x| String::from_utf8_lossy(&x.data).to_string())
                })
                .unwrap_or_default();
            (r.header.form_id, edid)
        })
        .collect();

    let mut shown = 0;
    let mut lights = 0usize;
    for (form_id, edid) in &lighs {
        match session.light_for(*form_id) {
            Some(l) => {
                lights += 1;
                if shown < 5 || l.spot || l.off_by_default {
                    println!(
                        "  {:<42} color={:?} radius={} fade={} fov={} spot={} off={}",
                        edid.trim_end_matches('\0'),
                        l.color,
                        l.radius,
                        l.fade,
                        l.fov,
                        l.spot,
                        l.off_by_default
                    );
                    shown += 1;
                }
            }
            None => println!("  {:<42} NOT READ", edid.trim_end_matches('\0')),
        }
    }
    println!("{lights} lights read");
}
