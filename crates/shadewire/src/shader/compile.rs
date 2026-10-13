//! The steps every back end shares: optimize (when asked), reflect, build the material's tree; the back end
//! prints it.

use super::ast::Module;
use super::compile_options::CompileOptions;
use super::material::{MaterialOptions, material};
use crate::optimize::optimize;
use crate::{BindingOptions, Dag, Error, Reflection, SourceInfo, reflect};

/// A compiled material: the generated code in the back end's language, what a host must bind, and the DAG the
/// code was generated from (optimized when `CompileOptions::optimize`).
#[derive(Clone, Debug)]
pub struct Compiled {
    pub code: String,
    pub reflection: Reflection,
    pub dag: Dag,
}

/// Optimizes (when asked), reflects and builds the material's tree.
pub fn lower(dag: &Dag, source: &SourceInfo, options: &CompileOptions) -> Result<(Module, Reflection, Dag), Error> {
    let dag = if options.optimize { optimize(dag) } else { dag.clone() };
    let bindings = BindingOptions {
        group: options.group,
        first_binding: options.first_binding,
        reserve_uniform_binding: options.reserve_uniform_binding,
    };
    let reflection = reflect(&dag, &bindings, source)?;
    let material_options =
        MaterialOptions { flip_v: options.flip_v, implicit_derivatives: options.implicit_derivatives };
    Ok((material(&dag, &reflection, &material_options), reflection, dag))
}

/// [`lower`], then `print` the tree.
pub fn compile_with(
    dag: &Dag,
    source: &SourceInfo,
    options: &CompileOptions,
    print: impl FnOnce(&Module, &Reflection) -> String,
) -> Result<Compiled, Error> {
    let (module, reflection, dag) = lower(dag, source, options)?;
    Ok(Compiled { code: print(&module, &reflection), reflection, dag })
}
