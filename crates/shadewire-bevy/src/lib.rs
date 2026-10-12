//! Shadewire materials in Bevy: each MaterialX graph becomes a `StandardMaterial` extension whose
//! main-pass fragment shader is generated per material.

mod error;
pub mod extension;
mod key;
mod plugin;
pub mod uniforms;

pub use error::ShadewireError;
pub use extension::ShadewireExtension;
pub use key::ShadewireKey;
pub use plugin::{ShadewireMaterial, ShadewirePlugin, ShadewireWhite};
