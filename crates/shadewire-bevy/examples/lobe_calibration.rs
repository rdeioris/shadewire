//! Checks that a closure tree's lobes light in Bevy's units: a white Lambert lobe (MaterialX
//! `oren_nayar_diffuse_bsdf`, roughness 0) and a white `StandardMaterial` (perceptual roughness 1, reflectance 0,
//! whose Burley diffuse is exactly Lambert at normal incidence) side by side, facing a directional light along
//! the view direction. Both must read `exposure × illuminance / π`, here 0.5 in linear light.
//!
//! `cargo run -p shadewire-bevy --example lobe_calibration`; exits with an error when either is off by more than
//! 1%.

use std::f32::consts::PI;

use bevy::camera::{Exposure, ScalingMode};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
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
/// Frames drawn before the shot, for the pipelines to compile.
const FRAMES: u32 = 8;

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
        .add_systems(Startup, setup)
        .add_systems(Update, shoot)
        .run()
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut shadewire: ResMut<Assets<ShadewireMaterial>>,
    mut assets: ShadewireAssets,
) {
    let exposure = 1.0 / (1.2 * 2f32.powf(EV100));
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
    commands.spawn((
        DirectionalLight { illuminance: EXPECTED * PI / exposure, shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(0.0, 0.0, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
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

fn shoot(mut commands: Commands, mut frames: Local<u32>) {
    *frames += 1;
    if *frames != FRAMES {
        return;
    }
    commands.spawn(Screenshot::primary_window()).observe(
        |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            let read = |x: u32| {
                let c = captured.image.get_color_at(x, SIZE / 2).expect("a pixel").to_linear();
                (c.red + c.green + c.blue) / 3.0
            };
            let (bevy, lobes) = (read(SIZE / 2), read(3 * SIZE / 2));
            println!("expected {EXPECTED:.4}  StandardMaterial {bevy:.4}  Lambert lobe {lobes:.4}");
            let off = |v: f32| (v - EXPECTED).abs() > 0.01 * EXPECTED;
            if off(bevy) || off(lobes) {
                eprintln!("error: off by more than 1%");
                exit.write(AppExit::error());
            } else {
                exit.write(AppExit::Success);
            }
        },
    );
}
