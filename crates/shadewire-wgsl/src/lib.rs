//! Shadewire WGSL back end: the material's shader tree (built by `shadewire::shader`) printed as WGSL, with its
//! reflection.

pub mod compile;
pub mod syntax;
#[cfg(test)]
mod tests;
#[cfg(any(test, feature = "validate"))]
pub mod validate;

pub use compile::{compile, emit};
pub use shadewire::shader::material::MaterialOptions;
pub use shadewire::shader::{CompileOptions, Compiled};
pub use syntax::{Wgsl, literal, print, wgsl_type};
