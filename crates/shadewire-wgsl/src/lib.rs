//! Shadewire WGSL back end: the material's shader tree (built by `shadewire::shader`) printed as WGSL, with its
//! reflection

pub mod compile;
#[cfg(feature = "translate")]
pub mod glsl_library;
#[cfg(feature = "translate")]
pub mod link;
pub mod reference_host;
pub mod syntax;
#[cfg(feature = "translate")]
pub mod target;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
#[cfg(feature = "translate")]
pub mod translate;

pub use compile::{compile, emit};
#[cfg(feature = "translate")]
pub use glsl_library::glsl_library;
#[cfg(feature = "translate")]
pub use link::link;
pub use shadewire::shader::material::MaterialOptions;
pub use shadewire::shader::{CompileOptions, Compiled};
pub use syntax::{Wgsl, literal, print, wgsl_type};
#[cfg(feature = "translate")]
pub use target::{GlslVersion, HlslShaderModel, MslVersion, Target};
#[cfg(feature = "translate")]
pub use translate::{Output, glsl_binding, translate};
