//! Shadewire Godot back end: material IR to a Godot 4 spatial shader (`.gdshader`) and reflection.
//!
//! The output is a complete `shader_type spatial;` file: uniforms with hints, the leaf library the
//! live nodes call, `shadewire_eval_surface` over a context struct that `fragment()` fills from
//! Godot's built-ins, and the mapping of the surface model onto Godot's outputs (`ALBEDO`,
//! `ROUGHNESS`, ...). Every field Godot cannot represent is listed in `reflection.approximations`.

pub mod compile;
pub mod emit;
pub mod leaf_library;
pub mod options;

pub use compile::{GdshaderOutput, compile};
pub use options::{GdshaderOptions, Transparency};
