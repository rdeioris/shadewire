use bevy::asset::uuid::Uuid;

use crate::extension::ShadewireExtension;

/// Pipeline key of a Shadewire material: the id of its generated fragment shader. Identical graphs
/// share one id, so they share pipelines. Bevy's combined extension key is `#[repr(packed)]` and
/// derives through `Copy`, so the key holds the `Uuid` the shader was inserted under, not a handle.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ShadewireKey {
    /// The `Uuid` the material's generated fragment shader was inserted under in `Assets<Shader>`; the
    /// extension's `specialize` swaps that shader in for the main pass.
    pub shader: Uuid,
}

impl From<&ShadewireExtension> for ShadewireKey {
    fn from(extension: &ShadewireExtension) -> Self {
        ShadewireKey { shader: extension.shader }
    }
}
