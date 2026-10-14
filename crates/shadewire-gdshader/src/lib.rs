//! Shadewire Godot back end: material IR to a Godot 4 spatial shader (`.gdshader`) and reflection.
//!
//! The output is a complete `shader_type spatial;` file. The material's code is the shared shader tree every
//! back end prints (`shadewire::shader`), with its support library; Godot adds named uniforms with inspector
//! hints, a context struct that `fragment()` fills from Godot's built-ins (stage built-ins are visible only in
//! the stage functions), and either
//!
//! - for a named model, the model mapped onto Godot's outputs (`ALBEDO`, `ROUGHNESS`, ...), Godot's BRDF lighting
//!   it, or
//! - for a closure tree (a pbrlib `surface`, or a named model opened through its graph), a `light()` that
//!   evaluates MaterialX's reference lobes per light ([`lobes`]).
//!
//! Every field or effect Godot cannot represent is listed in `reflection.approximations`.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/shadewire-mark.svg",
    html_favicon_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/png/shadewire-mark-64.png"
)]

pub mod context;
pub mod limits;
pub mod lobes;
mod module;
pub mod names;
pub mod options;
mod reserved;
pub mod rewrite;
pub mod surface;
pub mod syntax;
pub mod types;
mod uniforms;

pub use module::compile;
pub use options::{GdshaderOptions, Transparency};

use shadewire::{Dag, Reflection};

/// The models a host passes to the front end (`MtlxOptions::models`) to have named models mapped onto Godot's
/// outputs and lit by Godot's BRDF; a pbrlib `surface` still gets its lobes.
pub const MODELS: [&str; 7] = [
    "open_pbr_surface",
    "standard_surface",
    "gltf_pbr",
    "surface_unlit",
    "UsdPreviewSurface",
    "disney_principled",
    "surface",
];

/// The result of a compilation: the `.gdshader` text, what a host must bind, the Godot name of each reflection
/// uniform, and the DAG the shader was generated from (optimized when asked).
///
/// Godot binds by name: uniform `i` is the shader parameter `uniform_names[i]` (the authored name made a valid
/// identifier clear of reserved and generated names) and texture `t` is `shadewire_t<t.index>`, bound raw (the
/// shader converts colorspaces). Reflection's `group` and bindings mean nothing in Godot.
#[derive(Clone, Debug)]
pub struct GdshaderOutput {
    pub code: String,
    pub reflection: Reflection,
    pub uniform_names: Vec<String>,
    pub dag: Dag,
}

#[cfg(test)]
mod tests;
