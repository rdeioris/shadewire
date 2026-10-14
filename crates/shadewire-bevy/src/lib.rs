//! Shadewire materials in Bevy: each MaterialX graph becomes a `StandardMaterial` extension whose
//! main-pass fragment shader is generated per material. Forward rendering only: cameras with a deferred
//! prepass draw Shadewire meshes with Bevy's standard deferred shader.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/shadewire-mark.svg",
    html_favicon_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/png/shadewire-mark-64.png"
)]

mod assets;
mod built;
mod cache;
mod error;
mod extension;
mod host;
mod key;
mod layers;
mod lobes;
mod material;
mod models;
mod options;
mod plugin;
pub mod shader;
mod textures;
mod uniforms;

pub use assets::ShadewireAssets;
pub use built::Built;
pub use cache::{ShaderCache, ShaderHold, free_unused_shaders};
pub use error::ShadewireError;
pub use extension::ShadewireExtension;
pub use key::ShadewireKey;
pub use material::FromMtlx;
pub use options::BuildOptions;
pub use plugin::{ShadewireMaterial, ShadewirePlugin};
pub use shader::compile_options;
