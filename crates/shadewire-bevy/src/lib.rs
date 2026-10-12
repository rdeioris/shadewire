//! Shadewire materials in Bevy: each MaterialX graph becomes a `StandardMaterial` extension whose
//! main-pass fragment shader is generated per material. Forward rendering only: cameras with a deferred
//! prepass draw Shadewire meshes with Bevy's standard deferred shader.

mod error;
mod extension;
mod host;
mod key;
mod layers;
mod material;
mod models;
mod plugin;
pub mod shader;
mod textures;
mod uniforms;

pub use error::ShadewireError;
pub use extension::ShadewireExtension;
pub use key::ShadewireKey;
pub use material::FromMtlx;
pub use plugin::{ShadewireMaterial, ShadewirePlugin};
pub use shader::compile_options;
