//! Shadewire materials in Bevy: each MaterialX graph becomes a `StandardMaterial` extension whose
//! main-pass fragment shader is generated per material. Forward rendering only: cameras with a deferred
//! prepass draw Shadewire meshes with Bevy's standard deferred shader.

mod error;
pub mod extension;
pub mod host;
mod key;
mod material;
pub mod models;
mod plugin;
pub mod textures;
pub mod uniforms;
pub mod wesl;

pub use error::ShadewireError;
pub use extension::ShadewireExtension;
pub use key::ShadewireKey;
pub use material::{FromMtlx, shader_id};
pub use plugin::{ShadewireMaterial, ShadewirePlugin};
pub use wesl::compile_options;
