//! A MaterialX material on a glTF scene, or on a sphere and a cube.
//!
//! `cargo run -p shadewire-bevy --example gltf_mtlx -- [scene.glb|gltf] <material.mtlx> [--material NAME] [--screenshot out.png]`
//!
//! Drag with the left mouse button to orbit, scroll to zoom, `R` to recompile the `.mtlx`.
//! `--screenshot` saves the window a few frames after the scene is ready, then exits.

mod args;
mod orbit;
mod scene;
mod shot;
mod swap;

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::window::WindowResolution;
use shadewire_bevy::ShadewirePlugin;

use orbit::Orbit;
use scene::SceneFile;
use shot::{Ready, ShotPath};
use swap::{Source, Swaps};

fn main() -> AppExit {
    let args = match args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("error: {err}\n{}", args::USAGE);
            return AppExit::error();
        }
    };
    let source = Source { mtlx: args.mtlx, material: args.material };
    // Compile once before opening a window, so a bad file is reported without one.
    if let Err(err) = source.build(StandardMaterial::default(), &mut Assets::default(), &mut Assets::default()) {
        eprintln!("error: {}: {err}", source.mtlx.display());
        return AppExit::error();
    }
    let (asset_root, scene_file) = match &args.scene {
        Some(scene) => match split_scene(scene) {
            Ok((root, file)) => (root, Some(file)),
            Err(err) => {
                eprintln!("error: {err}");
                return AppExit::error();
            }
        },
        None => ("assets".to_string(), None),
    };

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin { file_path: asset_root, ..default() })
            .set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("shadewire: {}", source.mtlx.display()),
                    resolution: WindowResolution::new(1280, 720),
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins(ShadewirePlugin)
    .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
    .insert_resource(source)
    .init_resource::<Swaps>()
    .init_resource::<Ready>()
    .add_systems(Startup, setup)
    .add_systems(Update, (orbit::frame, orbit::orbit, swap::reload, shot::take, scene::watch).chain());
    if let Some(path) = args.screenshot {
        let _ = std::fs::remove_file(&path);
        app.insert_resource(ShotPath(path));
    }
    match scene_file {
        Some(file) => {
            app.insert_resource(SceneFile(file)).add_systems(Startup, scene::load).add_observer(scene::replace);
        }
        None => {
            app.add_systems(Startup, scene::fallback);
        }
    }
    app.run()
}

/// The asset root (the scene's directory, absolute) and the scene's file name in it.
fn split_scene(scene: &Path) -> Result<(String, String), String> {
    if !scene.is_file() {
        return Err(format!("no scene file {}", scene.display()));
    }
    let absolute = std::path::absolute(scene).map_err(|e| format!("{}: {e}", scene.display()))?;
    let dir = absolute.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let file = absolute.file_name().and_then(|f| f.to_str()).ok_or("the scene file name is not UTF-8")?;
    Ok((dir.to_string_lossy().into_owned(), file.to_string()))
}

fn setup(mut commands: Commands) {
    let orbit = Orbit { target: Vec3::ZERO, yaw: 0.0, pitch: -0.3, distance: 5.0 };
    commands.spawn((Camera3d::default(), orbit.transform(), orbit));
    commands.spawn((
        DirectionalLight { illuminance: 8000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(3.0, 5.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    println!("drag: orbit, scroll: zoom, R: recompile the .mtlx");
}
