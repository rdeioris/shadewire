pub mod context;
pub mod expressions;
mod module;
pub mod names;
pub mod ops;
mod reserved;
pub mod sample;
pub mod surface;
#[cfg(test)]
pub(crate) mod test_support;
pub mod types;
mod uniforms;

use shadewire::{Dag, Reflection};

use crate::options::GdshaderOptions;
pub use module::Assembled;
pub use names::UniformNames;

/// Emits the Godot shader for `dag`. Reflection says which uniforms and textures are live;
/// `names` are the uniforms' Godot names.
pub fn emit(dag: &Dag, reflection: &Reflection, names: &UniformNames, options: &GdshaderOptions) -> Assembled {
    module::assemble(dag, reflection, names, options)
}

#[cfg(test)]
mod tests;
