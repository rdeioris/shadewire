//! Compile MaterialX material graphs to WGSL surface-parameter functions

mod compile;
mod options;

pub use compile::{Compiled, compile_mtlx};
pub use options::CompileOptions;
pub use shadewire_ir::{Dag, Error, Reflection};
pub use shadewire_wgsl::translate::{Output, Target, translate};
pub use shadewire_wgsl::{link::link, reference_host};
