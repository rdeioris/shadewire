mod bindings;
#[allow(dead_code)] // used by the expression emitter (Task 20)
mod coerce;
mod module;
mod names;
mod reserved;
mod structs;
#[allow(dead_code)] // literal is used by the expression emitter (Task 20)
mod types;
mod uniforms;

use shadewire_ir::{Dag, Reflection};

#[derive(Clone, Debug, PartialEq)]
pub struct EmitOptions {
    /// Textures are uploaded top row first, MaterialX texcoords have V up.
    pub flip_v: bool,
}

impl Default for EmitOptions {
    fn default() -> Self {
        EmitOptions { flip_v: true }
    }
}

/// Emits the WGSL module for `dag`. Reflection says which uniforms and textures are live.
pub fn emit(dag: &Dag, reflection: &Reflection, options: &EmitOptions) -> String {
    module::assemble(dag, reflection, options)
}
