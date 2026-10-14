//! Checks that a closure tree's lobes light in Bevy's units: a white Lambert lobe (MaterialX
//! `oren_nayar_diffuse_bsdf`, roughness 0) and a white `StandardMaterial` (perceptual roughness 1, reflectance 0,
//! whose Burley diffuse is exactly Lambert at normal incidence) side by side, facing the camera, under one light
//! at a time:
//!
//! - a directional light along the view direction: both must read `exposure × illuminance / π`;
//! - the ambient light: both must read the same (Bevy scales a diffuse albedo by its split-sum fit, and the lobe
//!   path keeps that scale);
//! - a uniform environment map, filtered by Bevy at run time: both must read `exposure × intensity`.
//!
//! The expected value is 0.5 in linear light. `cargo run -p shadewire-bevy --example lobe_calibration`; exits with
//! an error when a value is off by more than 1% (2% for the filtered environment map).

use std::f32::consts::PI;

use bevy::asset::RenderAssetUsages;
use bevy::camera::{Exposure, ScalingMode};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::light::GeneratedEnvironmentMapLight;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::WindowResolution;
use shadewire_bevy::{BuildOptions, FromMtlx, ShadewireAssets, ShadewireMaterial, ShadewirePlugin};
use shadewire_mtlx::MtlxOptions;

const LAMBERT: &str = r#"<materialx version="1.39">
  <oren_nayar_diffuse_bsdf name="diffuse" type="BSDF">
    <input name="color" type="color3" value="1, 1, 1" />
    <input name="roughness" type="float" value="0" />
  </oren_nayar_diffuse_bsdf>
  <surface name="S" type="surfaceshader">
    <input name="bsdf" type="BSDF" nodename="diffuse" />
  </surface>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="S" />
  </surfacematerial>
</materialx>"#;

/// EV100 0: Bevy's exposure is `1 / (1.2 × 2^ev100)` (bevy_camera Exposure).
const EV100: f32 = 0.0;
const EXPECTED: f32 = 0.5;
const SIZE: u32 = 128;
/// Frames drawn in each scene before its shot: pipelines compile and the environment map is filtered.
const FRAMES: u32 = 12;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Scene {
    Directional,
    Ambient,
    Environment,
}

const SCENES: [Scene; 3] = [Scene::Directional, Scene::Ambient, Scene::Environment];

#[derive(Resource, Default)]
struct Run {
    scene: usize,
    frames: u32,
    shooting: bool,
    failed: bool,
}

fn exposure() -> f32 {
    1.0 / (1.2 * 2f32.powf(EV100))
}

fn main() -> AppExit {
    App::new()
        .add_plugins(DefaultPlugins.set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() }).set(
            WindowPlugin {
                primary_window: Some(Window {
                    title: "shadewire: lobe_calibration".into(),
                    resolution: WindowResolution::new(2 * SIZE, SIZE).with_scale_factor_override(1.0),
                    resizable: false,
                    ..default()
                }),
                ..default()
            },
        ))
        .add_plugins(ShadewirePlugin)
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(GlobalAmbientLight { brightness: 0.0, ..default() })
        .init_resource::<Run>()
        .add_systems(Startup, setup)
        .add_systems(Update, step)
        .run()
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut shadewire: ResMut<Assets<ShadewireMaterial>>,
    mut assets: ShadewireAssets,
) {
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed { width: 2.0, height: 1.0 },
            ..OrthographicProjection::default_3d()
        }),
        Exposure { ev100: EV100 },
        Tonemapping::None,
        DebandDither::Disabled,
        Msaa::Off,
        Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    let white = StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        metallic: 0.0,
        ..default()
    };
    commands.spawn((Mesh3d(quad.clone()), MeshMaterial3d(standard.add(white)), Transform::from_xyz(-0.5, 0.0, 0.0)));
    let options = BuildOptions {
        mtlx: MtlxOptions { models: shadewire::LOBE_MODELS.iter().map(|m| m.to_string()).collect(), ..default() },
        ..default()
    };
    let built = ShadewireMaterial::from_mtlx(LAMBERT, None, &options, default(), &mut assets).expect("the lobe builds");
    commands.spawn((Mesh3d(quad), MeshMaterial3d(shadewire.add(built.material)), Transform::from_xyz(0.5, 0.0, 0.0)));
}

/// A cubemap of radiance 1 in every direction.
fn uniform_cubemap() -> Image {
    let one = [0x00, 0x3c, 0x00, 0x3c, 0x00, 0x3c, 0x00, 0x3c]; // RGBA16F 1.0
    let size = Extent3d { width: 64, height: 64, depth_or_array_layers: 6 };
    let mut image =
        Image::new_fill(size, TextureDimension::D2, &one, TextureFormat::Rgba16Float, RenderAssetUsages::default());
    image.texture_view_descriptor =
        Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    image
}

/// Sets the scene up on its first frame and shoots it after `FRAMES`.
fn step(
    mut commands: Commands,
    mut run: ResMut<Run>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut images: ResMut<Assets<Image>>,
    camera: Single<Entity, With<Camera3d>>,
    lights: Query<Entity, With<DirectionalLight>>,
    mut exit: MessageWriter<AppExit>,
) {
    if run.shooting {
        return;
    }
    let Some(&scene) = SCENES.get(run.scene) else {
        exit.write(if run.failed { AppExit::error() } else { AppExit::Success });
        return;
    };
    run.frames += 1;
    if run.frames == 1 {
        for light in &lights {
            commands.entity(light).despawn();
        }
        ambient.brightness = 0.0;
        commands.entity(*camera).remove::<GeneratedEnvironmentMapLight>();
        match scene {
            Scene::Directional => {
                commands.spawn((
                    DirectionalLight {
                        illuminance: EXPECTED * PI / exposure(),
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
                ));
            }
            Scene::Ambient => ambient.brightness = EXPECTED / exposure(),
            Scene::Environment => {
                commands.entity(*camera).insert(GeneratedEnvironmentMapLight {
                    environment_map: images.add(uniform_cubemap()),
                    intensity: EXPECTED / exposure(),
                    ..default()
                });
            }
        }
    }
    if run.frames < FRAMES {
        return;
    }
    run.shooting = true;
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut run: ResMut<Run>| {
            let read = |x: u32| {
                let c = captured.image.get_color_at(x, SIZE / 2).expect("a pixel").to_linear();
                (c.red + c.green + c.blue) / 3.0
            };
            let (bevy, lobes) = (read(SIZE / 2), read(3 * SIZE / 2));
            let ok = match scene {
                Scene::Directional => near(bevy, EXPECTED, 0.01) && near(lobes, EXPECTED, 0.01),
                Scene::Ambient => bevy > 0.0 && near(lobes, bevy, 0.01),
                Scene::Environment => near(bevy, EXPECTED, 0.02) && near(lobes, EXPECTED, 0.02),
            };
            println!(
                "{:<12} expected {EXPECTED:.4}  StandardMaterial {bevy:.4}  Lambert lobe {lobes:.4}  {}",
                format!("{scene:?}"),
                if ok { "ok" } else { "OFF" }
            );
            run.failed |= !ok;
            run.scene += 1;
            run.frames = 0;
            run.shooting = false;
        },
    );
}

/// Whether `value` is within `tolerance` (relative) of `target`.
fn near(value: f32, target: f32, tolerance: f32) -> bool {
    (value - target).abs() <= tolerance * target
}
