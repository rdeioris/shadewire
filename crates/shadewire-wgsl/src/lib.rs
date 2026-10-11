//! WGSL emitter for Shadewire material DAGs

pub mod emit;
pub mod leaf_library;
pub mod link;
pub mod reference_host;
pub mod translate;

pub use emit::{emit, EmitOptions};
