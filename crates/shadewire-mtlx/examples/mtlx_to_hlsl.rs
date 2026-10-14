//! Compile a MaterialX file to an HLSL library.
//!
//! cargo run -p shadewire-mtlx --example mtlx_to_hlsl -- path/to/material.mtlx

use shadewire::shader::CompileOptions;
use shadewire_mtlx::{Resolver, to_ir};
use shadewire_nodes::LowerOptions;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: mtlx_to_hlsl <file.mtlx>")?;
    let text = std::fs::read_to_string(&path)?;

    // MaterialX -> IR -> HLSL: the model's struct, `shadewire_eval_surface()` filling it, the closure functions,
    // and declarations of the host functions it calls, which the host defines next to its entry point.
    let ir = to_ir(&text, Some(&path), None, &LowerOptions::default(), &Resolver::default())?;
    let compiled = shadewire_hlsl::compile(&ir.dag, &ir.source, &CompileOptions::default())?;
    println!("// model: {}", compiled.reflection.model);
    println!("{}", compiled.code);
    Ok(())
}
