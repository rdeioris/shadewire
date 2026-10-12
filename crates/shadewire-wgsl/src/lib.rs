//! WGSL emitter for Shadewire material DAGs

pub mod emit;
pub mod leaf_library;
#[cfg(feature = "translate")]
pub mod link;
pub mod reference_host;
#[cfg(feature = "translate")]
pub mod target;
#[cfg(feature = "translate")]
pub mod translate;

pub use emit::{EmitOptions, emit};
