//! The particle census: which particle blocks a game's meshes carry.
//!
//! S13-1. `NiPSysModifier` and its subclasses, the emitters and the particle
//! data are in the schema (`schemas/nif/base/particles.json`, 50 blocks) and
//! are read nowhere. This enumerates them per game, by modifier type, so the
//! size of the unimplemented particle stage is a number rather than a
//! guess — and so a modifier type no game ships can be left out of the
//! implementation deliberately.
//!
//! For each emitter it also records the parameters `ParticleEmitterParams`
//! sanitizes, so a value the sanitizer would clamp is visible before the
//! clamp is ever applied.
//!
//! Usage: `particlesurvey <game id> [limit files]`

use nvre_editor::game::ALL;
use nvre_nif::scene::{Decoded, ParticleEmitterParams};
use std::collections::BTreeMap;

/// Every block that is a `NiPSysModifier` or derives from one, plus the
/// emitters and the particle data.
fn is_particle(dec: &Decoded, i: usize) -> bool {
    dec.is_a(i, "NiPSysModifier")
        || dec.is_a(i, "NiPSysEmitter")
        || dec.is_a(i, "NiPSysData")
        || dec.is_a(i, "NiParticleSystem")
}

#[derive(Default)]
struct Counts {
    files: u64,
    unparsed: u64,
    /// Files holding at least one particle block.
    files_with_particles: u64,
    /// Particle blocks, by exact type name.
    by_type: BTreeMap<String, u64>,
    /// `NiPSysModifier` subclasses, by exact type name.
    modifiers: BTreeMap<String, u64>,
    /// Emitters whose parameters the sanitizer would clamp.
    clamped_emitters: u64,
    /// Emitters seen.
    emitters: u64,
    /// Emitters whose speed is negative in the file.
    negative_speed: u64,
    /// Emitters whose life span is zero in the file.
    zero_life_span: u64,
}

fn main() {
    let game_id = std::env::args().nth(1).expect("a game id");
    let limit: usize = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);

    let game = ALL.iter().find(|g| g.id == game_id).expect("a game");
    let data_dir = game.data_dir().expect("a data folder");
    let data = game.open_asset_data(&data_dir).expect("the asset data");

    let mut c = Counts::default();
    for p in data.vfs.paths(|p| p.to_lowercase().ends_with(".nif")) {
        if c.files as usize >= limit {
            break;
        }
        let Some(bytes) = data.read(&p) else { continue };
        let Ok(f) = nvre_nif::NifFile::parse(&bytes, Some(&data.schema)) else {
            c.unparsed += 1;
            continue;
        };
        c.files += 1;
        let dec = Decoded::new(&f, &bytes, &data.schema);
        let mut had_particles = false;

        for i in 0..f.blocks.len() {
            if !is_particle(&dec, i) {
                continue;
            }
            had_particles = true;
            let ty = dec
                .lineage
                .get(i)
                .and_then(|l| l.first())
                .cloned()
                .unwrap_or_default();
            *c.by_type.entry(ty.clone()).or_insert(0) += 1;
            if dec.is_a(i, "NiPSysModifier") {
                *c.modifiers.entry(ty.clone()).or_insert(0) += 1;
            }
            // An emitter's parameters, read the way the sanitizer reads
            // them, so a value it would change is visible as a count.
            let Some(Ok(b)) = dec.blocks.get(i) else {
                continue;
            };
            if !dec.is_a(i, "NiPSysEmitter") {
                continue;
            }
            c.emitters += 1;
            let num = |name: &str| b.num(name).unwrap_or(0.0) as f32;
            let params = ParticleEmitterParams {
                speed: num("speed"),
                speed_variation: num("speed_variation"),
                declination: num("declination"),
                declination_variation: num("declination_variation"),
                planar_angle: num("planar_angle"),
                planar_angle_variation: num("planar_angle_variation"),
                initial_radius: num("initial_radius"),
                radius_variation: num("radius_variation"),
                life_span: num("life_span"),
                life_span_variation: num("life_span_variation"),
                max_particles: ParticleEmitterParams::DEFAULT_MAX_PARTICLES,
            };
            if params.speed < 0.0 {
                c.negative_speed += 1;
            }
            if params.life_span <= 0.0 {
                c.zero_life_span += 1;
            }
            let sanitized = params.sanitized();
            if sanitized != params {
                c.clamped_emitters += 1;
            }
        }
        if had_particles {
            c.files_with_particles += 1;
        }
    }

    println!(
        "{game_id}: {} files, {} unparsed, {} with particles",
        c.files, c.unparsed, c.files_with_particles
    );
    println!("S13-1 particle blocks");
    let mut v: Vec<_> = c.by_type.into_iter().collect();
    v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (ty, n) in v.iter().take(30) {
        println!("  {n:8} {ty}");
    }
    println!("  NiPSysModifier subclasses:");
    let mut m: Vec<_> = c.modifiers.into_iter().collect();
    m.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (ty, n) in m.iter().take(30) {
        println!("    {n:8} {ty}");
    }
    println!("  emitters: {}", c.emitters);
    println!("  sanitizer would clamp: {}", c.clamped_emitters);
    println!("  negative speed: {}", c.negative_speed);
    println!("  zero life span: {}", c.zero_life_span);
}
