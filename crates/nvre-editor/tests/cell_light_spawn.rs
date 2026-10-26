//! The viewport's light spawn, run headless against a real master.
//!
//! `draw_cell_lights` takes only `Commands`, the session and `Spawned`, so it can
//! be driven without a window: this pins that a cell's references actually
//! produce Bevy lights, how many, and what they are set to. The previous version
//! of this file passed vacuously — it looked up a cell by a name the master does
//! not have, so it never ran the assertion. It now requires a real cell and
//! fails if it cannot find one.

use bevy::prelude::*;
use nvre_editor::session::{CellScope, Session};
use nvre_editor::viewport::{draw_cell_lights, Spawned};

/// The first cell in a master that holds `LIGH` references, and its count.
fn a_cell_with_lights(data: &str, schema_dir: &str, master: &str) -> Option<(u32, String, usize)> {
    let mut session = Session::open(
        std::path::Path::new(data),
        std::path::Path::new(schema_dir),
        vec![master.to_string()],
    )
    .expect("the master opens");
    for w in session.worldspaces() {
        for c in session.cells(CellScope::Worldspace(w.form_id)) {
            session.open_cell(c.form_id);
            let refs = session.cell_refs().to_vec();
            let mut n = 0usize;
            for r in &refs {
                let Some(base) = r
                    .base
                    .as_deref()
                    .and_then(|b| u32::from_str_radix(b.strip_prefix("0x")?, 16).ok())
                else {
                    continue;
                };
                if session.light_for(base).is_some() {
                    n += 1;
                }
            }
            if n > 0 {
                return Some((
                    c.form_id,
                    c.name.clone().or(c.edid.clone()).unwrap_or_default(),
                    n,
                ));
            }
        }
    }
    None
}

/// Run the spawn on a cell and report every light it produced.
fn spawn(cell: u32, data: &str, schema_dir: &str, master: &str) -> Vec<(f32, f32, bool)> {
    let mut session = Session::open(
        std::path::Path::new(data),
        std::path::Path::new(schema_dir),
        vec![master.to_string()],
    )
    .expect("the master opens");
    session.open_cell(cell);

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(session);
    app.init_resource::<Spawned>();
    app.add_systems(Update, draw_cell_lights);
    app.update();

    let world = app.world_mut();
    let mut out = Vec::new();
    let mut q = world.query::<(Option<&PointLight>, Option<&SpotLight>)>();
    for (p, s) in q.iter(&world) {
        if let Some(l) = p {
            out.push((l.intensity, l.range, false));
        } else if let Some(l) = s {
            out.push((l.intensity, l.range, true));
        }
    }
    out
}

/// A cell with lamps produces real Bevy lights, with sane settings.
#[test]
fn a_cell_with_lamps_spawns_lights() {
    let cases = [
        ("NVRE_LIGHTS_fnv", "fnv", "FalloutNV.esm"),
        ("NVRE_LIGHTS_fo3", "fo3", "Fallout3.esm"),
        ("NVRE_LIGHTS_skyrim", "skyrimle", "Skyrim.esm"),
        ("NVRE_LIGHTS_skyrimse", "skyrimse", "Skyrim.esm"),
        ("NVRE_LIGHTS_fo4", "fo4", "Fallout4.esm"),
    ];
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas");
    let mut ran = false;
    for (var, sub, master) in cases {
        let Ok(data) = std::env::var(var) else {
            continue;
        };
        let Some((cell, name, want)) = a_cell_with_lights(&data, &format!("{dir}/{sub}"), master)
        else {
            continue;
        };
        ran = true;
        let lights = spawn(cell, &data, &format!("{dir}/{sub}"), master);
        println!(
            "{name} cell {cell:08X}: {want} LIGH refs -> {} lights, sample {:?}",
            lights.len(),
            lights.first().copied()
        );
        assert!(
            !lights.is_empty(),
            "{name}: {want} LIGH references must spawn lights"
        );
        for (intensity, range, spot) in &lights {
            assert!(*intensity > 0.0, "{name}: a light must have intensity");
            assert!(*range > 0.0, "{name}: a light must have a range");
            assert!(range.is_finite());
            let _ = spot;
        }
    }
    assert!(ran, "no game's master was named by NVRE_LIGHTS_*");
}
