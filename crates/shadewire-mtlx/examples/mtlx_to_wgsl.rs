//! Compile a MaterialX file to WGSL.
//!
//! cargo run -p shadewire-mtlx --example mtlx_to_wgsl -- path/to/material.mtlx

use std::path::Path;

use shadewire_mtlx::{MtlxOptions, to_ir};
use shadewire_wgsl::{CompileOptions, compile};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: mtlx_to_wgsl <file.mtlx>")?;
    let text = std::fs::read_to_string(&path)?;

    // 1. Front end: MaterialX -> IR (a typed DAG computing the material's surface parameters).
    let ir = to_ir(
        &text,
        Some(Path::new(&path)), // names the file in errors; base for relative includes
        None,                   // material name, when the document has several
        &MtlxOptions::default(),
    )?;

    // 2. Back end: IR -> WGSL `fn sw_eval_surface() -> <ModelStruct>` plus reflection.
    let compiled = compile(&ir.dag, &ir.source, &CompileOptions::default())?;

    println!("// model: {}", compiled.reflection.model);
    println!("{}", compiled.wgsl);
    Ok(())
}
