//! Shader code as a typed syntax tree, shared by every back end: the material's code is built as a tree, the
//! support libraries are parsed into one, and each back end (WGSL, GLSL, HLSL, MSL) only prints it.

pub mod ast;
pub mod fixtures;
pub mod infer;
pub mod library;
pub mod material;
pub mod names;
pub mod parse;
pub mod print;
mod reserved;

pub use ast::*;
