//! The surface: which material, which shading model, and the fields it gets.

pub mod closure;
pub mod distill;
pub mod find;
pub mod finish;
pub mod mix;
pub mod models;

#[cfg(test)]
mod closure_tests;
#[cfg(test)]
mod diamond_tests;
#[cfg(test)]
mod distill_tests;
#[cfg(test)]
mod finish_tests;
#[cfg(test)]
mod tests;

pub use find::{MixInput, SurfaceSource};
pub use finish::ShaderNode;
pub use models::{CLOSURE_MODEL, KNOWN_MODELS, fields, struct_name};
