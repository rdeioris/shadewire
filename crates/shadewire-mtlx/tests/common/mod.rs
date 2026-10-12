//! The tests' composition of the MaterialX front end and the WGSL back end.

use std::path::Path;

use shadewire_ir::Error;
use shadewire_mtlx::{MtlxOptions, to_ir};
use shadewire_wgsl::{CompileOptions, Compiled, compile};

/// `to_ir`, then `compile`.
pub fn compile_mtlx(
    text: &str,
    file: Option<&Path>,
    material: Option<&str>,
    mtlx: &MtlxOptions,
    options: &CompileOptions,
) -> Result<Compiled, Error> {
    let ir = to_ir(text, file, material, mtlx)?;
    compile(&ir.dag, &ir.source, options)
}
