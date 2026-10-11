//! WGSL emitter for Shadewire material DAGs

pub mod emit;
pub mod leaf_library;

pub use emit::{emit, EmitOptions};
