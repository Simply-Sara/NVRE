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

use nvre_editor::{
    file_details_window, game, gizmo, launcher, object_editor, panels, session, size_info_window,
    viewport, windows,
};

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use clap::Parser;
use std::path::PathBuf;

/// NVRE's Fallout: New Vegas editor.
#[derive(Parser, Clone, Resource)]
#[command(name = "nvre-editor", version)]
struct Args {
    /// Record schema directory to read instead of the ones compiled into the
    /// binary. NVRE's own: upstream's layouts plus the records and subrecords
    /// they do not define (`schemas/README.md`). Defaults to the built-in
    /// schemas, which are the same layouts the repository holds.
    #[arg(long)]
    esm_schema: Option<PathBuf>,
    /// The plugin to edit. Defaults to the game's own master.
    #[arg(long)]
    plugin: Option<String>,
    /// Masters the saved plugin will depend on, in load order. Defaults to the
    /// game's own master and DLC list (`game.rs`).
    #[arg(long, num_args = 1..)]
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
    /// Exterior grid view radius (0 = 1x1, 1 = 3x3, 2 = 5x5 zone, 3 = 7x7).
    #[arg(long, default_value_t = 2)]
    radius: i32,
}

/// The game the launcher's dropdown starts on.
///
/// The editor opens to the launcher so the user can choose what to work on, so
/// this is only where the dropdown starts pointing. It is the first game with
/// an install, and it is not remembered between runs: the picker is the point
/// of the window, so a run does not carry a prior choice into it.
///
/// A machine with no install at all still opens the launcher, with the field
/// empty for the user to fill — the editor is a front door, and having nothing
/// to open yet is not a reason to refuse to open.
fn launcher_game() -> Result<&'static game::GameProfile, String> {
    let found: Vec<&'static game::GameProfile> = game::ALL
        .iter()
        .filter(|g| g.data_dir().is_some())
        .collect();
    match found.as_slice() {
        [] => Ok(&game::FNV),
        many => Ok(many[0]),
    }
}

/// The plugin `--check` opens, and the game it is.
///
/// A plugin is a path, and the game is detected from the folder it lives in —
/// the same detection a session does for itself. So `--check` needs no game
/// flag: point it at a plugin and the game follows.
fn check_target(args: &Args) -> Result<(std::path::PathBuf, &'static game::GameProfile), String> {
    let path = match &args.plugin {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            // No plugin named: use the master of whichever game has an
            // install. A machine with several installed is ambiguous, and
            // saying so is better than opening the wrong one.
            let found: Vec<&'static game::GameProfile> = game::ALL
                .iter()
                .filter(|g| g.data_dir().is_some())
                .collect();
            match found.as_slice() {
                [one] => one.data_dir().unwrap_or_default().join(one.master),
                [] => return Err("no game install found; name a plugin to check".to_string()),
                many => {
                    let names: Vec<&str> = many.iter().map(|g| g.id).collect();
                    return Err(format!(
                        "several games installed ({}); name a plugin to check",
                        names.join(", ")
                    ));
                }
            }
        }
    };
    let dir = path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let detected = game::GameProfile::detect(&dir);
    Ok((path, detected))
}

/// The record schemas: `--esm-schema` when named, otherwise the ones compiled
/// into the binary.
///
/// The built editor carries its schemas, so the default is no directory at
/// all. A directory named on the command line is read from disk instead,
/// which is how a developer points the editor at a layout they are still
/// writing. The game's own layers are laid over it either way, so naming the
/// base directory is enough.
fn resolve_schema_dir(args: &Args) -> PathBuf {
    args.esm_schema.clone().unwrap_or_default()
}

/// The masters the session will depend on, overridden by the flag.
///
/// The masters are read from the Data folder the plugin lives in, so a partial
/// install yields only what it holds and a mod master the user installed is
/// included.
fn resolve_masters(
    args: &Args,
    game: &game::GameProfile,
    data_dir: &std::path::Path,
) -> Vec<String> {
    if args.masters.is_empty() {
        game.masters_in(data_dir)
    } else {
        args.masters.clone()
    }
}

/// Keep the `Game` resource in step with the launcher's pick.
///
/// The launcher is the only thing that chooses a game, and it can change its
/// mind, so the resource the panels read is updated from it rather than
/// frozen at startup.
fn sync_game_resource(choice: Res<launcher::GameChoice>, mut game: ResMut<nvre_editor::Game>) {
    if game.0 != choice.game {
        game.0 = choice.game;
    }
}

/// Report what the session loaded, without opening a window.
fn check(game: &game::GameProfile, mut session: session::Session) {
    println!("game          {}", game.name);
    println!("plugin        {}", session.doc.name());
    println!("masters       {:?}", session.masters);
    println!("records       {}", session.doc.records().len());
    println!("cells         {}", session.index.cells.len());
    println!("references    {}", session.index.refs.len());
    println!("worldspaces   {}", session.index.worldspaces.len());
    // Anything staged for creation, which a save will write.
    for w in session.staged_worldspaces() {
        println!("new worldspace {:08X} {}", w.form_id, w.spec.edid);
    }
    for c in session.staged_cells() {
        let where_ = match (c.spec.world, c.spec.grid) {
            (Some(w), Some((x, y))) => format!("world {w:08X} grid {x}, {y}"),
            (Some(w), None) => format!("world {w:08X}"),
            _ => "interior".to_string(),
        };
        println!("new cell     {:08X} {} ({where_})", c.form_id, c.spec.edid);
    }
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
        let (path, profile) = match check_target(&args) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        };
        let schema_dir = resolve_schema_dir(&args);
        let masters = resolve_masters(
            &args,
            profile,
            path.parent().unwrap_or(std::path::Path::new(".")),
        );
        let mut session = match session::Session::open(&path, &schema_dir, masters) {
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
        return check(profile, session);
    }

    // The launcher starts on a game. Which one is the first with an install,
    // and the user picks another from the dropdown — the launcher does not
    // remember a prior pick, because the picker is the point of the window.
    let profile = match launcher_game() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    // The asset layer opens when a game is chosen, not before: the launcher is
    // the front door, and the game it picks decides which archives and which
    // NIF schema to open. Until then there is no `ArchivePlugin`, which is why
    // the launcher's own window draws only egui and no 3D viewport exists.
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
                // Named by the launcher's pick, which starts as the detected
                // game; `launcher::ui` retitles it when the pick changes.
                title: "Select a game".into(),
                // The launcher fills this window, so it is sized for the
                // launcher's contents rather than for a panel.
                resolution: (640, 520).into(),
                position: WindowPosition::new(IVec2::new(40, 40)),
                ..default()
            }),
            close_when_requested: false,
            ..default()
        }))
        .add_plugins(bevy_egui::EguiPlugin::default())
        // The NIF material's shaders and `MaterialPlugin`: identical for every
        // game, so registered here rather than waiting for the game to be
        // picked. The game's own half of `ArchivePlugin` — its archives, its
        // NIF schema, and the caches over them — is added by `AddAssets` once
        // the launcher has been through.
        .add_plugins(nvre_assets::nif_material::NifMaterialPlugin)
        // The viewport's hotbar draws a wireframe, and Bevy's wireframe is a
        // plugin of its own rather than part of `PbrPlugin`. Adding it here, at
        // startup like the NIF material, keeps the app's render-world plugins in
        // one place: on a GPU without `POLYGON_MODE_LINE` or immediates it logs
        // one warning and the button simply does nothing, which is better than
        // the button being wired to a plugin that is not there.
        .add_plugins(bevy::pbr::wireframe::WireframePlugin::default())
        .insert_resource(args.clone())
        .insert_resource(nvre_editor::Game(profile))
        // The `Game` resource is refreshed from the launcher's pick whenever it
        // changes, so every panel that names the game names the one being
        // edited rather than the one present at startup.
        .add_systems(
            Update,
            (
                sync_game_resource.run_if(in_state(EditorState::Launching)),
                // Before anything draws: this app has one egui context per window,
                // and a style lives on its context, so every one is visited. In
                // `Update` rather than an egui pass so it runs whether the window
                // is the primary context or a panel with its own schedule.
                windows::widen_scroll_bars,
            ),
        )
        .insert_resource(launcher::GameChoice::new(profile))
        .insert_resource(launcher::Launcher::for_game(profile))
        .insert_resource(panels::Filters {
            exterior_radius: args.radius,
            ..default()
        })
        .init_resource::<viewport::Spawned>()
        .init_resource::<windows::ViewportHotbar>()
        .init_resource::<gizmo::GizmoState>()
        .init_resource::<object_editor::ObjectEditorState>()
        .init_resource::<file_details_window::FileDetailsState>()
        .init_resource::<size_info_window::SizeInfoState>()
        // The camera: the viewport's own plugin registers its input systems. There
        // is no controller plugin to add, because there is no controller — the
        // camera is a free one and the viewport moves its `Transform` itself.
        .add_plugins(viewport::ViewportCameraPlugin)
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
                windows::setup_viewport_wireframe,
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
            windows::Panel::Objects,
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
        // The viewport's hotbar, in the viewport's own egui schedule. It draws
        // into the viewport camera's own context, so it is a schedule of its own
        // rather than a `PanelContext<N>` panel.
        .add_systems(
            windows::Panel::Viewport,
            windows::viewport_hotbar.run_if(in_state(EditorState::Editing)),
        )
        // Reads what the hotbar decided and puts it on the wireframe and the
        // ambient that the render world draws with. In `Update`, not the
        // viewport's egui schedule: both are render-world resources, and the
        // change has to land before the frame's extract.
        .add_systems(
            Update,
            (windows::apply_wireframe, windows::apply_ambient)
                .chain()
                .run_if(in_state(EditorState::Editing)),
        )
        .add_systems(
            object_editor::ObjectEditorSchedule,
            object_editor::ui.run_if(in_state(EditorState::Editing)),
        )
        // The File Details window is spawned from the launcher, so it runs in
        // both states: it is opened before a session exists.
        .add_systems(
            file_details_window::FileDetailsSchedule,
            file_details_window::ui,
        )
        .add_systems(size_info_window::SizeInfoSchedule, size_info_window::ui)
        // The File Details window is opened from the launcher, which is
        // `Launching`, so its systems run in both states. A state gate here was
        // the bug that made the Details button do nothing.
        .add_systems(
            Update,
            (
                file_details_window::poll_scan,
                file_details_window::manage_window,
                size_info_window::manage_window,
            )
                .run_if(in_state(EditorState::Editing).or_else(in_state(EditorState::Launching))),
        )
        .add_systems(
            Update,
            (
                viewport::draw_cell,
                // The game's own lights, spawned from the same references and the
                // same cell. After the models so a light is never one frame ahead
                // of the thing it lights.
                viewport::draw_cell_lights,
                // Before the camera is placed: a frame request has to be applied
                // in the same frame it arrives, or the view snaps a frame late.
                viewport::apply_frame_requests,
                viewport::frame_selected,
                gizmo::gizmo_mode_hotkeys,
                gizmo::gizmo_system,
                viewport::pick,
                viewport::flat_material_diagnostic,
                object_editor::manage_window,
                handle_window_close,
                // Last, because everything above can ask for a view — a cell
                // opening, a frame request, `F`, a double-click in `pick` — and
                // this is what puts the camera there.
                viewport::sync_focus,
            )
                .chain()
                .run_if(in_state(EditorState::Editing)),
        )
        // In both states, because the File Details window can be closed from
        // either: leaving `Launching` out would let it respawn every frame once
        // closed while the launcher is still up.
        .add_systems(
            Update,
            handle_window_close
                .run_if(in_state(EditorState::Launching).or_else(in_state(EditorState::Editing))),
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

/// Add the chosen game's asset resources to the running app.
///
/// `ArchivePlugin` is two halves: `NifMaterialPlugin`, which registers the NIF
/// material's shaders and a `MaterialPlugin` and is identical for every game, and
/// the `GameData` resource with its two caches, which are the game's own. The
/// first is added at startup; this command carries the second, because a plugin
/// is added to the `App` while a system only has `Commands`. It is queued from
/// `open_selected`, once the launcher has picked a game, and lands before the
/// next frame's systems run.
struct AddAssets(nvre_assets::GameData);

impl Command for AddAssets {
    type Out = ();

    fn apply(self, world: &mut World) {
        world.insert_resource(self.0);
        world.init_resource::<nvre_assets::texture::TextureCache>();
        world.init_resource::<nvre_assets::model::ModelCache>();
    }
}

/// Open the file the launcher chose, and move to the editor.
///
/// Runs while launching, so the moment a file is picked the session is built and
/// the state changes: the editor's systems start on the next frame, with the
/// session in place.
///
/// This is also where the game's asset layer is opened — its archives and NIF
/// schema are the game's, and the game is only known once the launcher has been
/// through. A failure here is reported through the launcher, which is still on
/// screen, rather than killing the process.
fn open_selected(
    mut commands: Commands,
    launcher: Res<launcher::Launcher>,
    args: Res<Args>,
    choice: Res<launcher::GameChoice>,
    mut next: ResMut<NextState<EditorState>>,
    mut events: MessageReader<nvre_editor::OpenRequested>,
) {
    // The launcher's Open button raises this; the event carries what to open.
    let game = choice.game;
    // The Data folder the picker names: the game's own install, or the one the
    // user typed into the field. There is no command-line fallback any more, so
    // a game with neither is reported rather than opened against nothing.
    let Some(data_dir) = choice.active_dir() else {
        commands.insert_resource(nvre_editor::OpenFailed(format!(
            "{}: no Data folder; type one into the launcher's folder field",
            game.name
        )));
        return;
    };
    let schema_dir = resolve_schema_dir(&args);
    for _ in events.read() {
        // The game's archives and NIF schema, opened once for the session.
        let assets = match game.open_asset_data(&data_dir) {
            Ok(g) => g,
            Err(e) => {
                commands.insert_resource(nvre_editor::OpenFailed(format!("{}: {}", game.name, e)));
                return;
            }
        };
        // A plugin is added to the `App`, and a system has `Commands`, so the
        // add is queued as a command: it lands before the next frame's systems
        // run, which is after the state has moved to `Editing`.
        commands.queue(AddAssets(assets));

        let res = match launcher.tab {
            launcher::LauncherTab::CreateNew => {
                let mut name = launcher.new_name.trim().to_string();
                if !name.ends_with(".esp")
                    && !name.ends_with(".esm")
                    && !name.ends_with(".ESP")
                    && !name.ends_with(".ESM")
                {
                    name.push_str(if launcher.new_is_master {
                        ".esm"
                    } else {
                        ".esp"
                    });
                }
                // An empty field is no value: the header then gets GECK's
                // `DEFAULT` author and no SNAM at all, which is what a plugin
                // created with nothing typed should look like.
                let author = Some(launcher.new_author.trim())
                    .filter(|a| !a.is_empty())
                    .map(str::to_string);
                let description = Some(launcher.new_description.trim())
                    .filter(|d| !d.is_empty())
                    .map(str::to_string);
                session::Session::create(
                    &name,
                    launcher.new_is_master,
                    &data_dir,
                    &schema_dir,
                    launcher.masters.clone(),
                    author.as_deref(),
                    description.as_deref(),
                )
            }
            launcher::LauncherTab::OpenExisting => {
                let Some(path) = launcher.active.clone() else {
                    continue;
                };
                session::Session::open(&path, &schema_dir, launcher.masters.clone())
            }
        };

        match res {
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

/// The windows a close request can be for, other than the primary one.
///
/// Grouped because `handle_window_close` had grown past what a system signature
/// should carry, and these three are one idea: the windows the editor opens for
/// itself, and what closing each means.
#[derive(bevy::ecs::system::SystemParam)]
struct CloseTargets<'w, 's> {
    /// The object editor's window, if it is open.
    object: Query<'w, 's, Entity, With<object_editor::ObjectEditorWindow>>,
    /// The File Details window, if it is open.
    details: Query<'w, 's, Entity, With<file_details_window::FileDetailsWindow>>,
    /// The Size Info window, if it is open.
    size_info: Query<'w, 's, Entity, With<size_info_window::SizeInfoWindow>>,
    /// The File Details state, whose request is cleared on close — otherwise
    /// `manage_window` would spawn the window straight back.
    details_state: Option<ResMut<'w, file_details_window::FileDetailsState>>,
    /// The Size Info state, whose `open` is cleared on close for the same reason.
    size_info_state: Option<ResMut<'w, size_info_window::SizeInfoState>>,
}

/// Handle window close requests: primary window exits app, object editor window closes.
fn handle_window_close(
    mut commands: Commands,
    mut closed: MessageReader<bevy::window::WindowCloseRequested>,
    mut app_exit: MessageWriter<AppExit>,
    primary: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut targets: CloseTargets,
    mut session: Option<ResMut<session::Session>>,
) {
    let primary_ent = primary.iter().next();
    let obj_ent = targets.object.iter().next();
    let details_ent = targets.details.iter().next();
    let size_info_ent = targets.size_info.iter().next();

    for ev in closed.read() {
        if Some(ev.window) == primary_ent {
            app_exit.write(AppExit::Success);
        } else if Some(ev.window) == details_ent {
            // Clearing the request is what stops `manage_window` respawning it.
            if let Some(s) = &mut targets.details_state {
                s.close();
            }
        } else if Some(ev.window) == size_info_ent {
            if let Some(s) = &mut targets.size_info_state {
                s.close();
            }
        } else if Some(ev.window) == obj_ent {
            if let Some(s) = &mut session {
                s.close_object_editor();
            }
        } else {
            commands.entity(ev.window).despawn();
        }
    }
}
