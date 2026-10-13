//! Shadewire WGSL back end: the material's shader tree (built by `shadewire::shader`) printed as WGSL, with its
//! reflection.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/shadewire-mark.svg",
    html_favicon_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/png/shadewire-mark-64.png"
)]

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
