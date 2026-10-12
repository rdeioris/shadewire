//! What the builder returns: the material and the reflection that names its uniforms and textures.

use shadewire::Reflection;

/// A built material with its reflection. Keep the reflection to set uniforms by name later with
/// [`ShadewireExtension::set_uniform`](crate::ShadewireExtension::set_uniform).
#[derive(Clone, Debug)]
pub struct Built<M> {
    pub material: M,
    pub reflection: Reflection,
}
