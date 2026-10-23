//! Render a real game model through the whole pipeline, in a window.
//!
//! This is the test that proves the asset layer works: it reads a model out
//! of the game's own archives, builds its mesh and its material, draws it
//! with the NIF material, and saves what the camera saw as a PNG.
//!
//! The window stays open until it is closed, so the result can be looked at.
//!
//! Usage: `render_test <game> <model path> [out.png]`
//!
//! The game is one of `fnv`, `fo3`, `skyrim` or `skyrimse`. The model path
//! is relative to the game's Data folder, with backslashes.
use bevy::app::AppExit;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{save_to_disk, Screenshot};
use bevy::window::ExitCondition;
use nvre_assets::model::ModelCtx;
use nvre_assets::GameData;
use std::path::PathBuf;

/// The Data folder of each game, found on this machine.
fn data_dir(game: &str) -> Option<PathBuf> {
    let candidates: &[&str] = match game {
        "fnv" => &["/home/sara/.local/share/Steam/steamapps/common/Fallout New Vegas/Data"],
        "fo3" => &["/home/sara/Games/BethesdaGames/Fallout3/Data"],
        "skyrim" => &["/home/sara/Games/BethesdaGames/Skyrim/Data"],
        "skyrimse" => &[
            "/home/sara/Games/BethesdaGames/Skyrim Special Edition/Data",
            "/home/sara/.local/share/Steam/steamapps/common/Skyrim Special Edition/Data",
        ],
        _ => return None,
    };
    candidates.iter().map(PathBuf::from).find(|p| p.exists())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let game = args.get(1).cloned().unwrap_or_else(|| "fnv".into());
    let model = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "meshes\\clutter\\bobblehead\\vaultboy.nif".into());
    let out = args.get(3).cloned().unwrap_or_else(|| "/tmp/nif-render.png".into());

    let data_dir = data_dir(&game).expect("the game's Data folder");
    println!("the data folder: {}", data_dir.display());
    println!("the model: {model}");

    App::new()
        .insert_resource(RenderTest {
            data_dir,
            schema_dir: PathBuf::from("schemas/nif"),
            model,
            out,
            loaded: false,
            saved: false,
            frames: 0,
        })
        .insert_resource(ClearColor(Color::srgb_u8(20, 22, 28)))
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "NVRE render test".into(),
                    resolution: (960, 720).into(),
                    ..default()
                }),
                // Do not exit for want of a window; the run ends when the
                // window is closed.
                exit_condition: ExitCondition::DontExit,
                ..default()
            }),
        )
        .add_plugins(nvre_assets::nif_material::NifMaterialPlugin)
        .init_resource::<nvre_assets::model::ModelCache>()
        .init_resource::<nvre_assets::texture::TextureCache>()
        .add_systems(Startup, setup)
        .add_systems(Update, (load_model, take_screenshot))
        .run();
}

/// What the test needs.
#[derive(Resource)]
struct RenderTest {
    data_dir: PathBuf,
    schema_dir: PathBuf,
    model: String,
    out: String,
    /// The model has been drawn.
    loaded: bool,
    /// The frame has been saved.
    saved: bool,
    /// How many frames have passed since the model was drawn.
    frames: u32,
}

/// A camera, a light, and the game's files.
fn setup(mut commands: Commands, test: Res<RenderTest>) {
    // The game's files and the schema.
    let data = GameData::open(&test.data_dir, &test.schema_dir).expect("the game's files");
    commands.insert_resource(data);

    // The camera is placed once the model is known, so it frames it.
    commands.spawn((
        Camera3d::default(),
        Tonemapping::Linear,
        Transform::default(),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 20_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(2.0, 3.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            ..default()
        },
        Transform::from_xyz(-2.0, 1.5, -2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        PointLight {
            intensity: 400_000.0,
            ..default()
        },
        Transform::from_xyz(0.0, 2.0, 6.0),
    ));

}

/// Load the model, and draw it.
fn load_model(
    mut commands: Commands,
    mut models: ModelCtx,
    mut test: ResMut<RenderTest>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
) {
    if test.loaded {
        return;
    }
    test.loaded = true;
    let path = test.model.replace('/', "\\");
    let Some(model) = models.model(&path) else {
        println!("the model {path} did not load");
        return;
    };
    println!(
        "the model: {} nodes, {} shapes, bounds {:?}",
        model.nodes.len(),
        model.shapes.len(),
        model.bounds
    );
    for (i, s) in model.shapes.iter().enumerate() {
        println!("  shape {i}: {:?}", s.name);
    }
    for s in &model.shapes {
        // The base a prop stands on has no texture of its own; it is drawn
        // as a plain surface, which is not what this test is about. An editor
        // marker has no texture either, and it is exactly what this test is
        // about when a mesh's marker is the thing being looked at.
        if s.texture.is_none() && !s.editor_marker {
            println!(
                "  shape {}: {} triangles, no texture — not drawn",
                s.triangles, s.triangles
            );
            continue;
        }
        commands.spawn((
            Mesh3d(s.mesh.clone()),
            MeshMaterial3d(s.material.clone()),
            Transform::from_matrix(s.transform.into()),
        ));
    }
    // Point the camera at the model, from far enough away to see all of it.
    // The bounds are of the whole model, which includes the base that is
    // not drawn, so the camera is pulled in a little.
    let (lo, hi) = model.bounds.unwrap_or((Vec3::ZERO, Vec3::ONE));
    let centre = (lo + hi) / 2.0;
    // The camera is far enough back that the whole model fits, with room
    // to spare: the diagonal is the largest distance the model reaches.
    let radius = (hi - lo).length() / 2.0;
    let distance = radius * 3.5 + 1.0;
    // The model stands along z, so the camera is to the side of it, a
    // little above its middle: looking down the length of a model shows
    // it end-on.
    let eye = centre + Vec3::new(distance * 0.6, radius * 0.3, 0.0);
    for mut camera in &mut cameras {
        *camera = Transform::from_translation(eye).looking_at(centre, Vec3::Y);
    }
    println!(
        "the model is at {:?}..{:?}, the camera is at {:?}",
        lo, hi, eye
    );
}

/// Take the frame, once the model has been drawn.
fn take_screenshot(
    mut commands: Commands,
    mut test: ResMut<RenderTest>,
    keys: Res<ButtonInput<KeyCode>>,
    textures: Res<nvre_assets::texture::TextureCache>,
) {
    if test.saved {
        // The window stays open until it is closed.
        if keys.just_pressed(KeyCode::Escape) {
            commands.write_message(AppExit::Success);
        }
        return;
    }
    test.frames += 1;
    // Let the shaders compile and the model settle before the frame is
    // taken.
    if test.frames < 30 {
        return;
    }
    test.saved = true;
    let out = test.out.clone();
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(out));
    println!("the frame is being saved to {}", test.out);
    for f in textures.failures() {
        println!("  the texture {f} did not load");
    }
    println!("press escape to close the window");
}
