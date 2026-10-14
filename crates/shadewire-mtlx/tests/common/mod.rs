//! The tests' composition of the MaterialX front end and the WGSL back end.

use std::path::Path;

use shadewire::Error;
use shadewire::shader::{CompileOptions, Compiled};
use shadewire_mtlx::{Resolver, to_ir};
use shadewire_nodes::IrOptions;
use shadewire_wgsl::compile;

/// `to_ir`, then `compile`.
pub fn compile_mtlx(
    text: &str,
    file: Option<&Path>,
    material: Option<&str>,
    lower: &IrOptions,
    options: &CompileOptions,
) -> Result<Compiled, Error> {
    let ir = to_ir(text, file.and_then(Path::to_str), material, lower, &Resolver::default())?;
    compile(&ir.dag, &ir.source, options)
}
