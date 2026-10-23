//! Resolve the object window's Name column against a real localized master.
//!
//! Run with the game installed to see the names the palette now shows:
//!
//! ```text
//! cargo run -p nvre-editor --example localized_names
//! ```
//!
//! Before the string tables were read, every name here was four bytes of
//! decoded string ID — `x&\x01` where "Dremora Valkynaz" belongs.

use nvre_editor::game;
use nvre_editor::session::Session;

fn main() {
    for game in [
        &game::FNV,
        &game::SKYRIM,
        &game::SKYRIM_SE,
        &game::FALLOUT_4,
    ] {
        let Some(data) = game.data_dir() else {
            println!("{}: not installed", game.name);
            continue;
        };
        let master = data.join(game.master);
        let schema = game.schema_dirs().0;
        let mut session = match Session::open(&master, &schema, vec![game.master.to_string()]) {
            Ok(s) => s,
            Err(e) => {
                println!("{}: cannot open: {e}", game.name);
                continue;
            }
        };
        println!("\n=== {} ({})", game.name, master.display());
        let groups = session.object_groups("").to_vec();
        let total: usize = groups.iter().map(|g| g.objects.len()).sum();
        let with_name = groups
            .iter()
            .flat_map(|g| &g.objects)
            .filter(|r| !r.name.is_empty())
            .count();
        let noise = groups
            .iter()
            .flat_map(|g| &g.objects)
            .filter(|r| r.name.chars().any(|c| c.is_control()))
            .count();
        println!(
            "{total} objects, {with_name} with a name, {noise} with control bytes in the name"
        );
        // A handful of creatures, by Editor ID, to read the actual strings.
        let wanted = [
            // Skyrim and Fallout 4: localized, so these are string IDs.
            "EncDremoraWarlock06",
            "EncDremoraMelee06",
            "DraugrRace",
            "Wolf",
            "Bear",
            // New Vegas: unlocalized, so its names are text in the record —
            // the path this change must not disturb.
            "Bighorner",
            "Cazador",
            "Gecko",
        ];
        for g in &groups {
            for row in &g.objects {
                if wanted.iter().any(|w| row.edid.starts_with(w)) {
                    println!("  {:<22} {:>10X}  {}", row.edid, row.fid, row.name);
                }
            }
        }
    }
}
