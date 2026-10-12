use bevy::pbr::{ExtendedMaterial, MaterialPlugin};
use bevy::prelude::*;

use crate::cache::{ShaderCache, free_unused_shaders};
use crate::extension::ShadewireExtension;

/// A Shadewire material: `StandardMaterial` extended with the generated shader and its bindings.
pub type ShadewireMaterial = ExtendedMaterial<StandardMaterial, ShadewireExtension>;

/// Registers `ShadewireMaterial`, the builder's `ShaderCache` and, in `Last`, `free_unused_shaders`. Unused texture slots stay `None`: Bevy binds its own white fallback
/// image for them. Forward rendering only: a camera with a deferred prepass draws opaque Shadewire
/// meshes through Bevy's standard deferred shader, so they show the base `StandardMaterial`.
pub struct ShadewirePlugin;

impl Plugin for ShadewirePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<ShadewireMaterial>::default())
            .init_resource::<ShaderCache>()
            .add_systems(Last, free_unused_shaders);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::image::ImagePlugin;

    #[test]
    fn plugin_adds_the_material() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), ImagePlugin::default(), ShadewirePlugin));
        assert!(app.world().get_resource::<Assets<ShadewireMaterial>>().is_some());
        assert!(app.world().get_resource::<ShaderCache>().is_some());
    }
}
