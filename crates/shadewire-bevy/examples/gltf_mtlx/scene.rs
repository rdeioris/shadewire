//! The scene: a glTF file whose `StandardMaterial`s are replaced once it is spawned, or a UV sphere and
//! a cube when no file is given.

use bevy::asset::LoadState;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

use crate::orbit::FramePending;
use crate::shot::{Ready, ShotPath};
use crate::swap::Builder;

/// The glTF scene being loaded, watched for a failed load.
#[derive(Resource)]
pub struct Scene(pub Handle<WorldAsset>);

/// Startup without a scene file: a sphere and a cube sharing one material over a default base.
pub fn fallback(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut builder: Builder,
    mut ready: ResMut<Ready>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(material) = builder.material(None, &StandardMaterial::default()) else {
        exit.write(AppExit::error());
        return;
    };
    let sphere = Sphere::new(1.0).mesh().uv(64, 32).with_generated_tangents();
    let cube = Cuboid::from_length(1.5).mesh().build().with_generated_tangents();
    let (Ok(sphere), Ok(cube)) = (sphere, cube) else { unreachable!("primitives have normals and UVs") };
    let rotation = Quat::from_euler(EulerRot::YXZ, 0.6, 0.4, 0.0);
    commands.spawn((Mesh3d(meshes.add(sphere)), MeshMaterial3d(material.clone()), Transform::from_xyz(-1.3, 0.0, 0.0)));
    commands.spawn((
        Mesh3d(meshes.add(cube)),
        MeshMaterial3d(material),
        Transform::from_xyz(1.3, 0.0, 0.0).with_rotation(rotation),
    ));
    ready.0 = true;
}

/// Startup with a scene file: spawns scene 0 of the file (the asset root is the file's directory).
pub fn load(mut commands: Commands, assets: Res<AssetServer>, file: Res<SceneFile>) {
    let handle: Handle<WorldAsset> = assets.load(GltfAssetLabel::Scene(0).from_asset(file.0.clone()));
    commands.spawn(WorldAssetRoot(handle.clone()));
    commands.insert_resource(Scene(handle));
}

/// The scene's file name, relative to the asset root.
#[derive(Resource)]
pub struct SceneFile(pub String);

/// Exits with an error when the scene fails to load.
pub fn watch(scene: Option<Res<Scene>>, assets: Res<AssetServer>, mut exit: MessageWriter<AppExit>) {
    let Some(scene) = scene else { return };
    if let LoadState::Failed(err) = assets.load_state(&scene.0) {
        eprintln!("error: {err}");
        exit.write(AppExit::error());
    }
}

/// A scene mesh still drawn with a `StandardMaterial`.
type SceneMesh = (&'static MeshMaterial3d<StandardMaterial>, Option<&'static Mesh3d>);

/// On `WorldInstanceReady`: every mesh with a `StandardMaterial` gets the MaterialX material built on a
/// clone of it (one per base material), and tangents when it has normals and UVs but no tangents.
pub fn replace(
    ready_event: On<WorldInstanceReady>,
    mut commands: Commands,
    (children, meshes_in_scene): (Query<&Children>, Query<SceneMesh>),
    standard: Res<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut builder: Builder,
    (mut exit, shot): (MessageWriter<AppExit>, Option<Res<ShotPath>>),
) {
    let mut replaced = 0;
    for entity in children.iter_descendants(ready_event.entity) {
        let Ok((base, mesh)) = meshes_in_scene.get(entity) else { continue };
        let Some(base_material) = standard.get(base.id()) else { continue };
        let Some(handle) = builder.material(Some(base.id()), base_material) else {
            exit.write(AppExit::error());
            return;
        };
        if let Some(mut mesh) = mesh.and_then(|m| meshes.get_mut(m.id())) {
            add_tangents(&mut mesh);
        }
        commands.entity(entity).remove::<MeshMaterial3d<StandardMaterial>>().insert(MeshMaterial3d(handle));
        replaced += 1;
    }
    println!("replaced the material of {replaced} glTF meshes");
    if replaced == 0 && shot.is_some() {
        eprintln!("error: the scene has no mesh with a StandardMaterial to replace; no screenshot");
        exit.write(AppExit::error());
        return;
    }
    commands.insert_resource(FramePending);
}

/// Generates mikktspace tangents for a mesh that has normals and UVs but no tangents. Meshes whose data
/// already went to the GPU only, or that cannot get tangents, keep the shader's screen-space frame.
fn add_tangents(mesh: &mut Mesh) {
    let has = |mesh: &Mesh, attribute| mesh.try_contains_attribute(attribute).unwrap_or(true);
    if has(mesh, Mesh::ATTRIBUTE_TANGENT) || !has(mesh, Mesh::ATTRIBUTE_NORMAL) || !has(mesh, Mesh::ATTRIBUTE_UV_0) {
        return;
    }
    if let Err(err) = mesh.generate_tangents() {
        warn!("no tangents generated: {err}");
    }
}
