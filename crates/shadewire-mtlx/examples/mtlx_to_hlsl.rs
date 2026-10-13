//! Compile a MaterialX file to HLSL.
//!
//! cargo run -p shadewire-mtlx --example mtlx_to_hlsl -- path/to/material.mtlx

use std::path::Path;

use shadewire::model_struct_name;
use shadewire_mtlx::{MtlxOptions, to_ir};
use shadewire_wgsl::{CompileOptions, HlslShaderModel, Output, Target, compile, link, reference_host, translate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: mtlx_to_hlsl <file.mtlx>")?;
    let text = std::fs::read_to_string(&path)?;

    // 1. MaterialX -> IR -> WGSL function filling the model's parameter struct, plus reflection.
    let ir = to_ir(&text, Some(Path::new(&path)), None, &MtlxOptions::default())?;
    let compiled = compile(&ir.dag, &ir.source, &CompileOptions::default())?;
    println!("// model: {}", compiled.reflection.model);

    // 2. Add host functions (context reads, transforms) and an entry point.
    //    The reference host is a minimal stand-in; a real renderer passes its own prelude.
    let struct_name = model_struct_name(&compiled.reflection.model);
    let prelude = reference_host::prelude(&compiled.reflection, &struct_name);
    let linked = link(&compiled.code, &prelude)?;

    // 3. WGSL -> HLSL through naga.
    let Output::Text(hlsl) = translate(&linked, Target::Hlsl(HlslShaderModel::Sm6_0), None)? else {
        unreachable!("HLSL is text");
    };
    println!("{hlsl}");
    Ok(())
}
