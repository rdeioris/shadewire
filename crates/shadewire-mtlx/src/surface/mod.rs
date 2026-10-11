//! The surface: which material, which shading model, and the fields it gets.

pub mod find;
pub mod mix;
pub mod models;

#[cfg(test)]
mod tests;

pub use find::{MixInput, SurfaceSource};
pub use models::{fields, struct_name, PHASE_ONE_MODELS};
