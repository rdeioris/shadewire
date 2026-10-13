//! Batch renderer for the material library regression harness (`tools/matlib`): each MaterialX file of a
//! list on a sphere, framed like the library's preview renders, saved as a square PNG.
//!
//! `cargo run -p shadewire-bevy --example matlib_render --release -- <list.tsv> <results.tsv> [--size N]`
//!
//! Each line of the list is `<material.mtlx>\t<out.png>[\t<uv scale>]`; the document's material (it must
//! have one) is built, on a sphere whose texture coordinates are multiplied by the scale (1 by default). Each
//! entry appends `start\t<out.png>` to the results before it is built, then `ok\t<out.png>` once the
//! shot is saved or `error\t<out.png>\t<message>` when the material does not build. A run that dies on
//! an entry leaves its `start` line last, so the caller can record the crash and resume after it.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::WindowResolution;
use shadewire_bevy::{BuildOptions, FromMtlx, ShadewireAssets, ShadewireMaterial, ShadewirePlugin};

const USAGE: &str = "usage: matlib_render <list.tsv> <results.tsv> [--size N]";

/// Frames drawn with a material before its shot.
const FRAMES: u32 = 4;

/// Vertical field of view; the sphere's silhouette spans `FILL` of it (a little cropped, as on the library's
/// previews).
const FOV: f32 = 30.0 * std::f32::consts::PI / 180.0;
const FILL: f32 = 1.03;

/// The backdrop of the library's previews (sRGB 43, 43, 43).
const BACKDROP: Color = Color::srgb_u8(43, 43, 43);

/// One line of the list.
#[derive(Clone)]
struct Entry {
    mtlx: PathBuf,
    out: PathBuf,
    uv_scale: f32,
}

#[derive(Resource)]
struct Batch {
    entries: Vec<Entry>,
    /// The sphere for each UV scale used so far.
    spheres: Vec<(f32, Handle<Mesh>)>,
    results: File,
    next: usize,
    phase: Phase,
}

enum Phase {
    /// Build the next entry's material.
    Load,
    /// Draw frames with it; the count so far.
    Draw(u32),
    /// The screenshot was requested; waiting for it to be saved.
    Capture,
}

impl Batch {
    fn record(&mut self, line: &str) {
        writeln!(self.results, "{line}").and_then(|_| self.results.flush()).expect("cannot write the results");
    }
}

/// The sphere every material goes on.
#[derive(Component)]
struct Subject;

fn main() -> AppExit {
    let (list, results, size) = match parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("error: {err}\n{USAGE}");
            return AppExit::error();
        }
    };
    let entries = match std::fs::read_to_string(&list).map_err(|e| e.to_string()).and_then(|text| entries(&text)) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("error: {}: {err}", list.display());
            return AppExit::error();
        }
    };
    let results = match OpenOptions::new().create(true).append(true).open(&results) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("error: {}: {err}", results.display());
            return AppExit::error();
        }
    };

    App::new()
        .add_plugins(DefaultPlugins.set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() }).set(
            WindowPlugin {
                primary_window: Some(Window {
                    title: "shadewire: matlib_render".into(),
                    resolution: WindowResolution::new(size, size).with_scale_factor_override(1.0),
                    resizable: false,
                    ..default()
                }),
                ..default()
            },
        ))
        .add_plugins(ShadewirePlugin)
        .insert_resource(ClearColor(BACKDROP))
        .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
        .insert_resource(Batch { entries, spheres: Vec::new(), results, next: 0, phase: Phase::Load })
        .add_systems(Startup, setup)
        .add_systems(Update, step)
        .run()
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<(PathBuf, PathBuf, u32), String> {
    let mut positional = Vec::new();
    let mut size = 512;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--size" => {
                size = args.next().and_then(|s| s.parse().ok()).ok_or("--size needs a number of pixels")?;
            }
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    match <[PathBuf; 2]>::try_from(positional) {
        Ok([list, results]) => Ok((list, results, size)),
        Err(p) => Err(format!("expected a list and a results file, got {} paths", p.len())),
    }
}

/// The entries of a list file.
fn entries(text: &str) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match line.split('\t').collect::<Vec<_>>()[..] {
            [mtlx, out] => entries.push(Entry { mtlx: mtlx.into(), out: out.into(), uv_scale: 1.0 }),
            [mtlx, out, scale] => {
                let uv_scale = scale.parse().map_err(|_| format!("bad UV scale in: {line}"))?;
                entries.push(Entry { mtlx: mtlx.into(), out: out.into(), uv_scale });
            }
            _ => return Err(format!("expected <material.mtlx>\\t<out.png>[\\t<uv scale>]: {line}")),
        }
    }
    Ok(entries)
}

fn setup(mut commands: Commands) {
    let distance = 1.0 / (FILL * FOV / 2.0).sin();
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: FOV, ..default() }),
        Transform::from_xyz(0.0, 0.0, distance).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight { illuminance: 8000.0, ..default() },
        Transform::from_xyz(-3.0, 4.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn(Subject);
}

/// A unit sphere made of a cube's six faces, each an `n` x `n` grid pushed out to the sphere, with UVs 0..1
/// across every face: the library's previews map textures this way (one texture tile spans the face
/// toward the camera, no pinching at poles), not with a latitude-longitude sphere. The UVs are multiplied
/// by `uv_scale`: the previews tile textures at a scale of their own, which the harness calibrates.
fn cube_sphere(n: u32, uv_scale: f32) -> Mesh {
    // (outward normal, right, up) of each face; right x up = normal, so triangles wind outward.
    let faces = [
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        (Vec3::X, Vec3::NEG_Z, Vec3::Y),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::NEG_Z),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
    ];
    let (mut positions, mut normals, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (normal, right, up) in faces {
        let first = positions.len() as u32;
        for j in 0..=n {
            for i in 0..=n {
                let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
                let point = (normal + right * (2.0 * u - 1.0) + up * (2.0 * v - 1.0)).normalize();
                positions.push(point.to_array());
                normals.push(point.to_array());
                uvs.push([u * uv_scale, (1.0 - v) * uv_scale]);
            }
        }
        for j in 0..n {
            for i in 0..n {
                let a = first + j * (n + 1) + i;
                let (b, c, d) = (a + 1, a + n + 2, a + n + 1);
                indices.extend([a, b, c, a, c, d]);
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

fn step(
    mut commands: Commands,
    mut batch: ResMut<Batch>,
    subject: Single<Entity, With<Subject>>,
    mut assets: ShadewireAssets,
    mut materials: ResMut<Assets<ShadewireMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut exit: MessageWriter<AppExit>,
) {
    match batch.phase {
        Phase::Load => {
            let Some(Entry { mtlx, out, uv_scale }) = batch.entries.get(batch.next).cloned() else {
                exit.write(AppExit::Success);
                return;
            };
            batch.record(&format!("start\t{}", out.display()));
            match ShadewireMaterial::from_mtlx_file(&mtlx, &BuildOptions::default(), default(), &mut assets) {
                Ok(built) => {
                    let material = built.material;
                    // The previous material's handle drops here, and its textures with it.
                    let sphere = match batch.spheres.iter().find(|(scale, _)| *scale == uv_scale) {
                        Some((_, sphere)) => sphere.clone(),
                        None => {
                            let mesh = cube_sphere(64, uv_scale).with_generated_tangents();
                            let sphere = meshes.add(mesh.expect("the cube sphere has normals and UVs"));
                            batch.spheres.push((uv_scale, sphere.clone()));
                            sphere
                        }
                    };
                    commands.entity(*subject).insert((Mesh3d(sphere), MeshMaterial3d(materials.add(material))));
                    batch.phase = Phase::Draw(0);
                }
                Err(err) => {
                    let message = err.to_string().replace(['\t', '\r', '\n'], " ");
                    batch.record(&format!("error\t{}\t{message}", out.display()));
                    batch.next += 1;
                }
            }
        }
        Phase::Draw(frames) if frames + 1 < FRAMES => batch.phase = Phase::Draw(frames + 1),
        Phase::Draw(_) => {
            let out = batch.entries[batch.next].out.clone();
            let _ = std::fs::remove_file(&out);
            if let Some(dir) = out.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let mut save = save_to_disk(out.clone());
            commands.spawn(Screenshot::primary_window()).observe(
                move |captured: On<ScreenshotCaptured>, mut batch: ResMut<Batch>| {
                    save(captured);
                    let status = if out.is_file() { "ok" } else { "error" };
                    let detail = if out.is_file() { String::new() } else { "\tcannot save the screenshot".into() };
                    batch.record(&format!("{status}\t{}{detail}", out.display()));
                    batch.next += 1;
                    batch.phase = Phase::Load;
                },
            );
            batch.phase = Phase::Capture;
        }
        Phase::Capture => {}
    }
}
