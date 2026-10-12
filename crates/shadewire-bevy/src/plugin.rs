use bevy::asset::RenderAssetUsages;
use bevy::pbr::{ExtendedMaterial, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::extension::ShadewireExtension;

/// A Shadewire material: `StandardMaterial` extended with the generated shader and its bindings.
pub type ShadewireMaterial = ExtendedMaterial<StandardMaterial, ShadewireExtension>;

/// A 1x1 white image for unused texture slots.
#[derive(Resource, Clone, Debug)]
pub struct ShadewireWhite(pub Handle<Image>);

pub struct ShadewirePlugin;

impl Plugin for ShadewirePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<ShadewireMaterial>::default());
        let white = app.world_mut().resource_mut::<Assets<Image>>().add(white_image());
        app.insert_resource(ShadewireWhite(white));
    }
}

fn white_image() -> Image {
    let size = Extent3d { width: 1, height: 1, depth_or_array_layers: 1 };
    Image::new_fill(size, TextureDimension::D2, &[255; 4], TextureFormat::Rgba8Unorm, RenderAssetUsages::RENDER_WORLD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::image::ImagePlugin;

    #[test]
    fn plugin_adds_the_material_and_a_white_image() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), ImagePlugin::default(), ShadewirePlugin));
        let white = app.world().resource::<ShadewireWhite>().0.clone();
        let image = app.world().resource::<Assets<Image>>().get(&white).unwrap();
        assert_eq!(image.data.as_deref(), Some(&[255u8, 255, 255, 255][..]));
        assert!(app.world().get_resource::<Assets<ShadewireMaterial>>().is_some());
    }
}
