//! The stores the builder writes to, borrowed together by a system.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::shader::Shader;

use crate::cache::ShaderCache;

/// What [`FromMtlx`](crate::FromMtlx) writes: textures to `images`, generated shaders to `shaders` and
/// their holds to `cache` (a resource of `ShadewirePlugin`).
#[derive(SystemParam)]
pub struct ShadewireAssets<'w> {
    pub images: ResMut<'w, Assets<Image>>,
    pub shaders: ResMut<'w, Assets<Shader>>,
    pub cache: ResMut<'w, ShaderCache>,
}
