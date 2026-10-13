# shadewire-bevy

MaterialX materials in [Bevy](https://bevyengine.org) 0.20, through [Shadewire](https://github.com/rdeioris/shadewire): a `ShadewireMaterial` is a
`StandardMaterial` (which keeps its alpha mode, lighting and shadows) extended with a fragment shader generated for
the material's graph. Identical graphs share one shader and pipeline whatever their values.

```rust
use bevy::prelude::*;
use shadewire_bevy::{FromMtlx, ShadewireMaterial, ShadewirePlugin};

fn main() {
    App::new().add_plugins((DefaultPlugins, ShadewirePlugin)).add_systems(Startup, spawn).run();
}

fn spawn(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut shaders: ResMut<Assets<Shader>>,
         mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<ShadewireMaterial>>) -> Result {
    let marble = ShadewireMaterial::from_mtlx_file("marble.mtlx", None, default(), &mut images, &mut shaders)?;
    let sphere = Sphere::new(1.0).mesh().uv(64, 32).with_generated_tangents()?;
    commands.spawn((Mesh3d(meshes.add(sphere)), MeshMaterial3d(materials.add(marble))));
    Ok(())
}
```

Forward rendering; up to 8 textures and 1 KiB of uniforms per material. Needs Rust 1.97 (Bevy's minimum).

Part of [Shadewire](https://github.com/rdeioris/shadewire), a compiler for material graphs: a front end turns a material description into a typed
IR, back ends turn it into shader code plus reflection that says what the host must bind.
