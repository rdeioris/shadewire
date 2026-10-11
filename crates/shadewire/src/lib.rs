//! Compile MaterialX material graphs to WGSL surface-parameter functions

mod compile;
mod options;

pub use compile::{compile_mtlx, Compiled};
pub use options::CompileOptions;
pub use shadewire_ir::{Dag, Error, Reflection};
pub use shadewire_wgsl::translate::{translate, Output, Target};
pub use shadewire_wgsl::{link::link, reference_host};
