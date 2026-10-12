//! The back end: any front end's IR to WGSL and reflection.

use shadewire_ir::optimize::optimize;
use shadewire_ir::{BindingOptions, Dag, Error, Reflection, SourceInfo, reflect};

use crate::compile_options::CompileOptions;
use crate::emit::{EmitOptions, emit};

/// The result of a compilation: the generated module, what a host must bind, and the DAG
/// the module was emitted from (optimized when `CompileOptions::optimize`).
#[derive(Clone, Debug)]
pub struct Compiled {
    pub wgsl: String,
    pub reflection: Reflection,
    pub dag: Dag,
}

/// Optimize (when asked), reflect, emit.
pub fn compile(dag: &Dag, source: &SourceInfo, options: &CompileOptions) -> Result<Compiled, Error> {
    let dag = if options.optimize { optimize(dag) } else { dag.clone() };
    let bindings = BindingOptions { group: options.group, first_binding: options.first_binding };
    let reflection = reflect(&dag, &bindings, source)?;
    let wgsl = emit(&dag, &reflection, &EmitOptions { flip_v: options.flip_v });
    Ok(Compiled { wgsl, reflection, dag })
}
