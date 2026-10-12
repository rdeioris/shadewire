//! Compile MaterialX material graphs to WGSL surface-parameter functions

mod compile;
mod options;

pub use compile::{Compiled, compile_mtlx};
pub use options::CompileOptions;
pub use shadewire_ir::{Dag, Error, Reflection, model_struct_name};
#[cfg(feature = "translate")]
pub use shadewire_wgsl::link::link;
pub use shadewire_wgsl::reference_host;
#[cfg(feature = "translate")]
pub use shadewire_wgsl::target::{GlslVersion, HlslShaderModel, MslVersion, Target};
#[cfg(feature = "translate")]
pub use shadewire_wgsl::translate::{Output, translate};
