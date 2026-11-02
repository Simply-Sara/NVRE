//! Benchmark WastelandNV with full worldspace and terrain loaded.
//!
//! Loads FalloutNV.esm into a session, enables full terrain and whole-worldspace
//! radius (-1), drives the viewport until all models, terrain quadrants, and lights
//! are fully spawned, and measures steady-state frame timings across benchmark camera angles.
//!
//! Usage:
//!   NVRE_DATA=/path/to/Data cargo run --release -p nvre-editor --example bench_worldspace
//!
//! Optional environment variables:
//!   BENCH_CELL=Goodsprings      (default: Goodsprings)
//!   BENCH_RADIUS=-1             (default: -1 for full worldspace, or e.g. 2 for 5x5 zone)
//!   BENCH_WARMUP=100            (warmup frames before sampling, default: 100)
//!   BENCH_FRAMES=300            (steady-state frames to measure, default: 300)

use bevy::app::AppExit;
use bevy::camera::RenderTarget;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowRef};
use nvre_assets::nif_material::NifMaterialPlugin;
use nvre_editor::game::GameProfile;
use nvre_editor::panels::Filters;
use nvre_editor::session::Session;
use nvre_editor::viewport::{self, LoadTimings, Spawned, ViewPose, ViewportCamera, ViewportFocus};
use nvre_editor::windows::{self, ViewportHotbar};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Resource)]
#[allow(dead_code)]
struct BenchConfig {
    cell_query: String,
    radius: i32,
    warmup_frames: usize,
    sample_frames: usize,
}

#[derive(Resource, Default)]
struct PerFrameTimings {
    frame_start: Option<Instant>,
    update_start: Option<Instant>,
    update_dur: f64,
    samples: Vec<FrameTimeBreakdown>,
}

#[derive(Clone, Copy, Default, Debug)]
struct FrameTimeBreakdown {
    total_ms: f64,
    update_ecs_ms: f64,
    render_gpu_wait_ms: f64,
}

#[derive(Resource)]
struct BenchState {
    phase: BenchPhase,
    start_time: Instant,
    last_log: Instant,
    warmup_count: usize,
    frame_samples: Vec<f64>, // frame delta in ms
    last_frame_instant: Option<Instant>,
    breakdown_samples: Vec<FrameTimeBreakdown>,
}

#[derive(Debug, PartialEq, Eq)]
enum BenchPhase {
    Loading,
    Warmup,
    MeasuringWorld,
    MeasuringSky,
    Finished,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_str = std::env::var("NVRE_DATA").unwrap_or_else(|_| {
        "/home/sara/.local/share/Steam/steamapps/common/Fallout New Vegas/Data".into()
    });
    let data_dir = PathBuf::from(&data_str);
    if !data_dir.exists() {
        eprintln!("Error: Data dir '{data_str}' does not exist. Set NVRE_DATA=<path>.");
        std::process::exit(1);
    }

    let cell_query = std::env::var("BENCH_CELL").unwrap_or_else(|_| "Goodsprings".into());
    let radius: i32 = std::env::var("BENCH_RADIUS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(-1);
    let warmup_frames: usize = std::env::var("BENCH_WARMUP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let sample_frames: usize = std::env::var("BENCH_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    println!("============================================================");
    println!(" NVRE WORLDSPACE BENCHMARK PROBE");
    println!(" Data path:      {}", data_dir.display());
    println!(" Initial cell:   {cell_query}");
    println!(
        " Exterior radius: {radius} ({} )",
        if radius < 0 {
            "Full Worldspace"
        } else {
            "Local Zone"
        }
    );
    println!(" Warmup frames:  {warmup_frames}");
    println!(" Sample frames:  {sample_frames}");
    println!("============================================================");

    let master_path = data_dir.join("FalloutNV.esm");
    let schema_dir = Path::new("schemas/fnv");
    println!("Loading FalloutNV.esm into Session...");
    let t0 = Instant::now();
    let mut session = Session::open(&master_path, schema_dir, vec!["FalloutNV.esm".to_string()])
        .map_err(|e| format!("Failed to open session: {e}"))?;
    println!("Session loaded in {:?}", t0.elapsed());

    // Resolve target cell
    let cell_fid = session
        .index
        .cells
        .values()
        .find(|c| {
            c.edid
                .as_deref()
                .is_some_and(|e| e.eq_ignore_ascii_case(&cell_query))
                || c.name
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(&cell_query))
                || format!("{:08X}", c.form_id).eq_ignore_ascii_case(&cell_query)
        })
        .map(|c| c.form_id)
        .ok_or_else(|| format!("Cell '{cell_query}' not found in index"))?;

    println!("Opening cell {cell_fid:08X}...");
    session.open_cell(cell_fid);

    let game = GameProfile::detect(&data_dir);
    println!("Opening game asset data archives ({:?})...", game.name);
    let assets = game
        .open_asset_data(&data_dir)
        .map_err(|e| format!("Asset data error: {e}"))?;

    let mut app = App::new();

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "NVRE Benchmark Probe".into(),
            resolution: (1280, 800).into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(NifMaterialPlugin)
    .add_plugins(bevy::pbr::wireframe::WireframePlugin::default())
    .add_plugins(viewport::ViewportCameraPlugin)
    .add_message::<nvre_editor::FrameRequested>();

    app.insert_resource(BenchConfig {
        cell_query,
        radius,
        warmup_frames,
        sample_frames,
    });
    app.insert_resource(BenchState {
        phase: BenchPhase::Loading,
        start_time: Instant::now(),
        last_log: Instant::now(),
        warmup_count: 0,
        frame_samples: Vec::with_capacity(sample_frames * 2),
        last_frame_instant: None,
        breakdown_samples: Vec::with_capacity(sample_frames * 2),
    });
    app.init_resource::<PerFrameTimings>();

    app.insert_resource(assets);
    app.init_resource::<nvre_assets::texture::TextureCache>();
    app.init_resource::<nvre_assets::model::ModelCache>();
    app.insert_resource(session);
    app.insert_resource(Filters {
        exterior_radius: radius,
        full_terrain: true,
        ..default()
    });
    app.init_resource::<Spawned>();
    app.init_resource::<LoadTimings>();
    app.init_resource::<ViewportHotbar>();

    // Initial setup
    app.add_systems(Startup, setup_bench_scene);

    app.add_systems(PreUpdate, frame_start_system);

    // Main viewport driving & benchmark loop
    app.add_systems(
        Update,
        (
            viewport::apply_cell_lighting,
            viewport::draw_cell,
            viewport::coarse_cell_culling,
            viewport::draw_cell_lights,
            viewport::apply_frame_requests,
            viewport::sync_focus,
            benchmark_step,
        )
            .chain(),
    );

    app.add_systems(PostUpdate, frame_end_system);

    app.run();

    Ok(())
}

fn frame_start_system(mut timings: ResMut<PerFrameTimings>) {
    let now = Instant::now();
    if let Some(prev) = timings.frame_start {
        let total = prev.elapsed().as_secs_f64() * 1000.0;
        let update = timings.update_dur;
        let gpu_wait = (total - update).max(0.0);
        timings.samples.push(FrameTimeBreakdown {
            total_ms: total,
            update_ecs_ms: update,
            render_gpu_wait_ms: gpu_wait,
        });
    }
    timings.frame_start = Some(now);
    timings.update_start = Some(now);
}

fn frame_end_system(mut timings: ResMut<PerFrameTimings>) {
    if let Some(start) = timings.update_start {
        timings.update_dur = start.elapsed().as_secs_f64() * 1000.0;
    }
}

fn setup_bench_scene(mut commands: Commands, primary_window: Query<Entity, With<PrimaryWindow>>) {
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.7, 0.75, 0.8),
        brightness: 800.0,
        ..default()
    });

    let primary_ent = primary_window.iter().next();

    // Spawn 3D camera explicitly rendering to the primary window if available
    let mut camera = commands.spawn((
        Camera3d::default(),
        Tonemapping::None,
        windows::msaa_from_setting(),
        Projection::Perspective(PerspectiveProjection {
            far: 1_500_000.0,
            near: 1.0,
            ..default()
        }),
        ViewportCamera,
    ));

    if let Some(ent) = primary_ent {
        camera.insert(RenderTarget::Window(WindowRef::Entity(ent)));
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn benchmark_step(
    mut exit: MessageWriter<AppExit>,
    mut state: ResMut<BenchState>,
    config: Res<BenchConfig>,
    spawned: Res<Spawned>,
    mut focus: ResMut<ViewportFocus>,
    timings: Res<LoadTimings>,
    mut per_frame: ResMut<PerFrameTimings>,
    time: Res<Time>,
    meshes_query: Query<(), With<Mesh3d>>,
    lights_query: Query<(), Or<(With<PointLight>, With<SpotLight>, With<DirectionalLight>)>>,
) {
    let now = Instant::now();
    let dt_ms = if let Some(last) = state.last_frame_instant {
        last.elapsed().as_secs_f64() * 1000.0
    } else {
        time.delta_secs_f64() * 1000.0
    };
    state.last_frame_instant = Some(now);

    match state.phase {
        BenchPhase::Loading => {
            if state.last_log.elapsed() >= Duration::from_secs(2) {
                state.last_log = now;
                println!(
                    "  [Loading...] Spawned refs: {} | Failed: {} | Models loaded: {} | In-flight: {} | Models done: {} | Lights done: {}",
                    spawned.refs.len(),
                    spawned.failed.len(),
                    spawned.loaded_models.len(),
                    spawned.in_flight_models.len(),
                    spawned.models_finished,
                    spawned.lights_finished
                );
            }

            if spawned.models_finished && spawned.lights_finished {
                let load_duration = state.start_time.elapsed();
                println!("------------------------------------------------------------");
                println!(" Worldspace fully loaded in {:.2?}", load_duration);
                println!("------------------------------------------------------------");
                println!(" LOAD TIME BREAKDOWN:");
                println!(
                    "   - Terrain Generation & Spawn:   {:>8.2?} ({:>5.1}%)",
                    timings.terrain_spawn,
                    (timings.terrain_spawn.as_secs_f64() / load_duration.as_secs_f64()) * 100.0
                );
                println!(
                    "   - Exterior Refs Query/Build:    {:>8.2?} ({:>5.1}%)",
                    timings.exterior_refs_query,
                    (timings.exterior_refs_query.as_secs_f64() / load_duration.as_secs_f64())
                        * 100.0
                );
                println!(
                    "   - Base -> NIF Path Resolution:  {:>8.2?} ({:>5.1}%)",
                    timings.base_model_paths_resolve,
                    (timings.base_model_paths_resolve.as_secs_f64() / load_duration.as_secs_f64())
                        * 100.0
                );
                println!(
                    "   - NIF Read/Decode/Material Load:{:>8.2?} ({:>5.1}%)",
                    timings.nif_models_load,
                    (timings.nif_models_load.as_secs_f64() / load_duration.as_secs_f64()) * 100.0
                );
                let nif_load_sec = timings.nif_models_load.as_secs_f64().max(0.0001);
                println!(
                    "       * BSA Archive Decompression: {:>8.2?} ({:>5.1}% of NIF load)",
                    timings.nif_bsa_read,
                    (timings.nif_bsa_read.as_secs_f64() / nif_load_sec) * 100.0
                );
                println!(
                    "       * NIF Schema / AST Parsing:  {:>8.2?} ({:>5.1}% of NIF load)",
                    timings.nif_parse_ast,
                    (timings.nif_parse_ast.as_secs_f64() / nif_load_sec) * 100.0
                );
                println!(
                    "       * Mesh & Tangents Gen:       {:>8.2?} ({:>5.1}% of NIF load)",
                    timings.nif_mesh_build,
                    (timings.nif_mesh_build.as_secs_f64() / nif_load_sec) * 100.0
                );
                println!(
                    "       * Texture BSA Read & DDS:    {:>8.2?} ({:>5.1}% of NIF load)",
                    timings.texture_load,
                    (timings.texture_load.as_secs_f64() / nif_load_sec) * 100.0
                );
                println!(
                    "       * Material & Asset Creation: {:>8.2?} ({:>5.1}% of NIF load)",
                    timings.material_build,
                    (timings.material_build.as_secs_f64() / nif_load_sec) * 100.0
                );
                println!(
                    "   - Reference Entity Spawn:       {:>8.2?} ({:>5.1}%)",
                    timings.ref_entities_spawn,
                    (timings.ref_entities_spawn.as_secs_f64() / load_duration.as_secs_f64())
                        * 100.0
                );
                println!(
                    "   - Lights Resolution & Spawn:    {:>8.2?} ({:>5.1}%)",
                    timings.lights_spawn,
                    (timings.lights_spawn.as_secs_f64() / load_duration.as_secs_f64()) * 100.0
                );
                let measured_total = timings.terrain_spawn
                    + timings.exterior_refs_query
                    + timings.base_model_paths_resolve
                    + timings.nif_models_load
                    + timings.ref_entities_spawn
                    + timings.lights_spawn;
                let other_time = load_duration.saturating_sub(measured_total);
                println!(
                    "   - Frame pacing & other systems: {:>8.2?} ({:>5.1}%) [over {} frames]",
                    other_time,
                    (other_time.as_secs_f64() / load_duration.as_secs_f64()) * 100.0,
                    timings.total_loading_frames
                );
                println!("------------------------------------------------------------");
                println!(
                    " Total 3D mesh instances:   {}",
                    meshes_query.iter().count()
                );
                println!(
                    " Total active scene lights: {}",
                    lights_query.iter().count()
                );
                println!(" Total spawned references:  {}", spawned.refs.len());
                println!(" Total terrain quadrants:   {}", spawned.terrain.len());
                println!("------------------------------------------------------------");
                println!("Starting warmup phase ({} frames)...", config.warmup_frames);

                // Set initial camera view pose (overview looking across world)
                let tgt = focus.target;
                viewport::request_view(
                    &mut focus,
                    ViewPose {
                        target: tgt,
                        distance: 25000.0,
                        pitch: Some(-0.35),
                        yaw: Some(0.0),
                    },
                );

                state.phase = BenchPhase::Warmup;
                state.warmup_count = 0;
                per_frame.samples.clear();
            }
        }
        BenchPhase::Warmup => {
            state.warmup_count += 1;
            if state.warmup_count >= config.warmup_frames {
                println!("Warmup complete. Measuring TEST 1: Full World View (Facing Horizon)...");
                state.phase = BenchPhase::MeasuringWorld;
                state.frame_samples.clear();
                state.breakdown_samples.clear();
                per_frame.samples.clear();
            }
        }
        BenchPhase::MeasuringWorld => {
            state.frame_samples.push(dt_ms);
            while let Some(sample) = per_frame.samples.pop() {
                state.breakdown_samples.push(sample);
            }
            if state.frame_samples.len() >= config.sample_frames {
                print_stats(
                    "TEST 1: FULL HORIZON VIEW (High Overdraw & Meshes)",
                    &state.frame_samples,
                    &state.breakdown_samples,
                    meshes_query.iter().count(),
                    lights_query.iter().count(),
                );

                println!(
                    "\nSwitching camera: Measuring TEST 2: Sky/Empty View (Pitch Straight Up)..."
                );
                println!("(Isolates CPU ECS/culling overhead from GPU vertex/raster load)");
                let tgt = focus.target;
                viewport::request_view(
                    &mut focus,
                    ViewPose {
                        target: tgt,
                        distance: 25000.0,
                        pitch: Some(1.50), // Facing almost straight up into empty sky
                        yaw: Some(0.0),
                    },
                );
                state.phase = BenchPhase::MeasuringSky;
                state.frame_samples.clear();
                state.breakdown_samples.clear();
                per_frame.samples.clear();
            }
        }
        BenchPhase::MeasuringSky => {
            state.frame_samples.push(dt_ms);
            while let Some(sample) = per_frame.samples.pop() {
                state.breakdown_samples.push(sample);
            }
            if state.frame_samples.len() >= config.sample_frames {
                print_stats(
                    "TEST 2: SKY / OFF-FRUSTUM VIEW (Camera Pitched Up)",
                    &state.frame_samples,
                    &state.breakdown_samples,
                    meshes_query.iter().count(),
                    lights_query.iter().count(),
                );

                println!("============================================================");
                println!(" BENCHMARK COMPLETE");
                println!("============================================================");
                state.phase = BenchPhase::Finished;
                exit.write(AppExit::Success);
            }
        }
        BenchPhase::Finished => {}
    }
}

fn print_stats(
    title: &str,
    samples: &[f64],
    breakdowns: &[FrameTimeBreakdown],
    mesh_count: usize,
    light_count: usize,
) {
    if samples.is_empty() {
        return;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let count = sorted.len();
    let sum: f64 = sorted.iter().sum();
    let mean_ms = sum / count as f64;
    let mean_fps = 1000.0 / mean_ms;

    let p50_ms = sorted[(count as f64 * 0.50) as usize];
    let p95_ms = sorted[((count as f64 * 0.95) as usize).min(count - 1)];
    let p99_ms = sorted[((count as f64 * 0.99) as usize).min(count - 1)];
    let min_ms = sorted[0];
    let max_ms = sorted[count - 1];

    let low_1pct_fps = 1000.0 / p99_ms;
    let min_fps = 1000.0 / max_ms;
    let max_fps = 1000.0 / min_ms;

    println!("------------------------------------------------------------");
    println!(" {title}");
    println!(" Samples:     {count} frames");
    println!(" Active meshes: {mesh_count} | Active lights: {light_count}");
    println!(" Mean FPS:    {mean_fps:6.1} FPS  ({mean_ms:5.2} ms)");
    println!(" 1% Low FPS:  {low_1pct_fps:6.1} FPS  ({p99_ms:5.2} ms)");
    println!(" Min/Max FPS: {min_fps:6.1} / {max_fps:6.1} FPS");
    println!(" Frame time percentiles: p50={p50_ms:5.2}ms, p95={p95_ms:5.2}ms, p99={p99_ms:5.2}ms");

    if !breakdowns.is_empty() {
        let b_count = breakdowns.len() as f64;
        let avg_ecs: f64 = breakdowns.iter().map(|b| b.update_ecs_ms).sum::<f64>() / b_count;
        let avg_gpu: f64 = breakdowns.iter().map(|b| b.render_gpu_wait_ms).sum::<f64>() / b_count;
        println!(" FRAME TIME BREAKDOWN (Average):");
        println!(
            "   - CPU ECS & Update Schedule:   {:>6.2} ms ({:>4.1}%)",
            avg_ecs,
            (avg_ecs / mean_ms) * 100.0
        );
        println!(
            "   - GPU Render & Driver Sync:    {:>6.2} ms ({:>4.1}%)",
            avg_gpu,
            (avg_gpu / mean_ms) * 100.0
        );

        // Find the worst frame (spike frame) breakdown
        if let Some(worst) = breakdowns
            .iter()
            .max_by(|a, b| a.total_ms.partial_cmp(&b.total_ms).unwrap())
        {
            println!(" WORST FRAME SPIKE BREAKDOWN ({:.2} ms):", worst.total_ms);
            println!(
                "   - CPU ECS & Update Schedule:   {:>6.2} ms",
                worst.update_ecs_ms
            );
            println!(
                "   - GPU Render & Driver Sync:    {:>6.2} ms",
                worst.render_gpu_wait_ms
            );
        }
    }
    println!("------------------------------------------------------------");
}
