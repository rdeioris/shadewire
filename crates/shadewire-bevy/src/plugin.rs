use bevy::pbr::{ExtendedMaterial, MaterialPlugin};
use bevy::prelude::*;

use crate::extension::ShadewireExtension;

/// A Shadewire material: `StandardMaterial` extended with the generated shader and its bindings.
pub type ShadewireMaterial = ExtendedMaterial<StandardMaterial, ShadewireExtension>;

/// Registers `ShadewireMaterial`. Unused texture slots stay `None`: Bevy binds its own white fallback
/// image for them. Forward rendering only: a camera with a deferred prepass draws opaque Shadewire
/// meshes through Bevy's standard deferred shader, so they show the base `StandardMaterial`.
pub struct ShadewirePlugin;

impl Plugin for ShadewirePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<ShadewireMaterial>::default());
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
    }
}
