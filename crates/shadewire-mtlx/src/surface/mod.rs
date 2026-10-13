//! The surface: which material, which shading model, and the fields it gets.

pub mod closure;
pub mod find;
pub mod mix;
pub mod models;

#[cfg(test)]
mod closure_tests;
#[cfg(test)]
mod diamond_tests;
#[cfg(test)]
mod tests;

pub use find::{MixInput, SurfaceSource};
pub use models::{CLOSURE_MODEL, PHASE_ONE_MODELS, fields, struct_name};
