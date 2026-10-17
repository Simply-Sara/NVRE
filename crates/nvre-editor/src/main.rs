//! nvre-editor — a GECK-like editor for Fallout: New Vegas.
//!
//! Reads the game's own data (read-only) and writes new `.esp` plugins. The
//! format work is in `nvre-edit`; this binary is the window around it.
//!
//! The game is never written to: saving produces a new file wherever the user
//! asks.
//!
//! ## Window layout
//!
//! The editor is one window per thing: a main window that is the front door, a
//! viewport, and one window per panel. See [`nvre_editor::windows`] for why the
//! main window is the primary one and the viewport names its own render target.

use nvre_editor::{launcher, panels, session, viewport, windows};

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use clap::Parser;
use nvf_assets::ArchivePlugin;
use std::path::PathBuf;

/// NVForever's Fallout: New Vegas editor.
#[derive(Parser, Clone, Resource)]
#[command(name = "nvre-editor", version)]
struct Args {
    /// The game's Data folder (read-only). Defaults to $NVRE_DATA.
    #[arg(long, env = "NVRE_DATA")]
    data: PathBuf,
    /// Record schema directory.
    #[arg(
        long,
        default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../NVForever/schemas/fnv")
    )]
    esm_schema: PathBuf,
    /// NIF schema directory, for the viewport's models.
    #[arg(
        long,
        default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../NVForever/schemas/nif")
    )]
    nif_schema: PathBuf,
    /// The plugin to edit.
    #[arg(long, default_value = "FalloutNV.esm")]
    plugin: String,
    /// Masters the saved plugin will depend on, in load order.
    #[arg(long, num_args = 1.., default_values_t = vec!["FalloutNV.esm".to_string()])]
    masters: Vec<String>,
    /// Cell to open first: Editor ID or 0xFormID.
    #[arg(long)]
    cell: Option<String>,
    /// Load the session, report what it sees, and exit without a window. Lets
    /// the editor's data path be checked where no display is available.
    #[arg(long)]
    check: bool,
    /// Draw every model in flat magenta with a plain unlit material, instead of
    /// the game's NIF material. A diagnostic for a blank viewport: if the models
    /// appear, the NIF material or its textures are what is failing.
    #[arg(long)]
    flat_material: bool,
}

/// Report what the session loaded, without opening a window.
fn check(mut session: session::Session) {
    println!("plugin        {}", session.doc.name());
    println!("masters       {:?}", session.masters);
    println!("records       {}", session.doc.records().len());
    println!("cells         {}", session.index.cells.len());
    println!("references    {}", session.index.refs.len());
    println!("worldspaces   {}", session.index.worldspaces.len());
    if let Some(cell) = session.cell {
        let refs = session.cell_refs();
        println!("open cell     {cell:08X} with {} refs", refs.len());
        for r in refs.iter().take(5) {
            println!(
                "   {:08X} {} {}",
                r.form_id,
                r.sig,
                r.edid.as_deref().unwrap_or("-")
            );
        }
    }
}

fn main() {
    let args = Args::parse();

    // The launcher runs first: nothing is loaded until a file is chosen, the way
    // GECK asks what to work on before it opens anything. `--check` still loads
    // headlessly, which is how the data path is verified without a display.
    if args.check {
        let mut session = match session::Session::open(
            &args.data.join(&args.plugin),
            &args.esm_schema,
            args.masters.clone(),
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        };
        if let Some(cell) = &args.cell {
            let fid = u32::from_str_radix(cell.trim_start_matches("0x"), 16)
                .ok()
                .or_else(|| {
                    session
                        .index
                        .cells
                        .values()
                        .find(|c| c.edid.as_deref() == Some(cell.as_str()))
                        .map(|c| c.form_id)
                });
            if let Some(f) = fid {
                session.open_cell(f);
            } else {
                eprintln!("warning: no cell {cell}");
            }
        }
        return check(session);
    }

    let game = match nvf_assets::GameData::open(&args.data, &args.nif_schema) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    if args.flat_material {
        // The system reads the environment so it can be set without a rebuild.
        unsafe { std::env::set_var("NVRE_FLAT_MATERIAL", "1") };
    }

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            // The main window is the primary one, which is the window winit
            // creates during its `resumed` event. The viewport and the panels
            // are spawned after that and depend on it existing.
            primary_window: Some(Window {
                title: windows::MAIN_TITLE.into(),
                // The launcher fills this window, so it is sized for the
                // launcher's contents rather than for a panel.
                resolution: (560, 440).into(),
                position: WindowPosition::new(IVec2::new(40, 40)),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(bevy_egui::EguiPlugin::default())
        .add_plugins(ArchivePlugin(game))
        .insert_resource(args.clone())
        .insert_resource(nvre_editor::DataDir(args.data.clone()))
        .init_resource::<launcher::Launcher>()
        .init_resource::<panels::Filters>()
        .init_resource::<viewport::Spawned>()
        // Inserted at startup, not in `viewport::setup`: the debug panel draws
        // into its own schedule, which can run before that system has inserted
        // it, and a missing resource fails the schedule and drops the pass.
        .init_resource::<viewport::Orbit>()
        .init_state::<EditorState>()
        .add_message::<nvre_editor::OpenRequested>()
        .add_message::<nvre_editor::FrameRequested>()
        // The launcher draws into the primary context, which has to exist before
        // anything is opened.
        .add_systems(Startup, windows::setup_primary)
        // The launcher owns the main window until a plugin is opened.
        .add_systems(
            EguiPrimaryContextPass,
            launcher::ui.run_if(in_state(EditorState::Launching)),
        )
        .add_systems(
            Update,
            open_selected.run_if(in_state(EditorState::Launching)),
        )
        // The editor's systems only run once a plugin is open: they all read the
        // session, which does not exist before then. The windows are spawned on
        // entering the state, not in `PostStartup`: that schedule has already run
        // by the time a file is chosen, so gating it on the state left the panels
        // never spawning at all.
        .add_systems(
            OnEnter(EditorState::Editing),
            (
                windows::spawn_viewport,
                viewport::setup,
                spawn_session,
                windows::spawn_windows,
            )
                .chain(),
        )
        .add_systems(
            EguiPrimaryContextPass,
            panels::main_window.run_if(in_state(EditorState::Editing)),
        )
        // Each panel draws into its own window's egui context, in that window's
        // schedule. This is what makes the panels independent windows rather
        // than regions of one window.
        .add_systems(
            windows::Panel::Cells,
            panels::cells.run_if(in_state(EditorState::Editing)),
        )
        .add_systems(
            windows::Panel::References,
            panels::references.run_if(in_state(EditorState::Editing)),
        )
        .add_systems(
            windows::Panel::Properties,
            panels::properties.run_if(in_state(EditorState::Editing)),
        )
        .add_systems(
            windows::Panel::Place,
            panels::place.run_if(in_state(EditorState::Editing)),
        )
        .add_systems(
            windows::Panel::Status,
            panels::status.run_if(in_state(EditorState::Editing)),
        )
        .add_systems(
            windows::Panel::Debug,
            panels::debug.run_if(in_state(EditorState::Editing)),
        )
        .add_systems(
            Update,
            (
                viewport::draw_cell,
                // Before the camera is placed: a frame request has to be applied
                // in the same frame it arrives, or the view snaps a frame late.
                viewport::apply_frame_requests,
                viewport::orbit_input,
                viewport::orbit_camera,
                viewport::pick,
                viewport::flat_material_diagnostic,
            )
                .chain()
                .run_if(in_state(EditorState::Editing)),
        )
        .run();
}

/// Whether the launcher or the editor owns the window.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
enum EditorState {
    /// Nothing is loaded; the launcher is asking what to open.
    #[default]
    Launching,
    /// A plugin is open and the editor's windows are up.
    Editing,
}

/// Open the file the launcher chose, and move to the editor.
///
/// Runs while launching, so the moment a file is picked the session is built and
/// the state changes: the editor's systems start on the next frame, with the
/// session in place.
fn open_selected(
    mut commands: Commands,
    launcher: Res<launcher::Launcher>,
    args: Res<Args>,
    mut next: ResMut<NextState<EditorState>>,
    mut events: MessageReader<nvre_editor::OpenRequested>,
) {
    // The launcher's Open button raises this; the event carries what to open.
    for _ in events.read() {
        let Some(path) = launcher.active.clone() else {
            continue;
        };
        match session::Session::open(&path, &args.esm_schema, launcher.masters.clone()) {
            Ok(mut session) => {
                if let Some(cell) = &args.cell {
                    let fid = u32::from_str_radix(cell.trim_start_matches("0x"), 16)
                        .ok()
                        .or_else(|| {
                            session
                                .index
                                .cells
                                .values()
                                .find(|c| c.edid.as_deref() == Some(cell.as_str()))
                                .map(|c| c.form_id)
                        });
                    if let Some(f) = fid {
                        session.open_cell(f);
                    }
                }
                commands.insert_resource(session);
                next.set(EditorState::Editing);
                return;
            }
            Err(e) => {
                // Reported through the launcher, which is still on screen.
                commands.insert_resource(nvre_editor::OpenFailed(e));
                return;
            }
        }
    }
}

/// Place the session the launcher built.
///
/// `open_selected` builds it, but a resource inserted from one system is not
/// visible to systems in the same frame; this runs on entering `Editing`, by
/// which time it is in place.
fn spawn_session() {}
