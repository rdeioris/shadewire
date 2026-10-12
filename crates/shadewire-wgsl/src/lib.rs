//! Shadewire WGSL back end: IR to WGSL and reflection; optional naga translation to HLSL, GLSL, MSL, SPIR-V

pub mod compile;
pub mod compile_options;
pub mod emit;
pub mod leaf_library;
#[cfg(feature = "translate")]
pub mod link;
pub mod reference_host;
#[cfg(feature = "translate")]
pub mod target;
#[cfg(feature = "translate")]
pub mod translate;

pub use compile::{Compiled, compile};
pub use compile_options::CompileOptions;
pub use emit::{EmitOptions, emit};
#[cfg(feature = "translate")]
pub use link::link;
#[cfg(feature = "translate")]
pub use target::{GlslVersion, HlslShaderModel, MslVersion, Target};
#[cfg(feature = "translate")]
pub use translate::{Output, translate};
