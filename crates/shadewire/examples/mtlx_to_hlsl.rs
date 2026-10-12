//! Compile a MaterialX file to HLSL.
//!
//! cargo run -p shadewire --example mtlx_to_hlsl -- path/to/material.mtlx

use std::path::Path;

use shadewire::{
    CompileOptions, HlslShaderModel, Output, Target, compile_mtlx, link, model_struct_name, reference_host, translate,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: mtlx_to_hlsl <file.mtlx>")?;
    let text = std::fs::read_to_string(&path)?;

    // 1. MaterialX -> WGSL function filling the model's parameter struct, plus reflection.
    let compiled = compile_mtlx(&text, Some(Path::new(&path)), None, &CompileOptions::default())?;
    println!("// model: {}", compiled.reflection.model);

    // 2. Add host functions (context reads, transforms) and an entry point.
    //    The reference host is a minimal stand-in; a real renderer passes its own prelude.
    let struct_name = model_struct_name(&compiled.reflection.model);
    let prelude = reference_host::prelude(&compiled.reflection, &struct_name);
    let linked = link(&compiled.wgsl, &prelude)?;

    // 3. WGSL -> HLSL through naga.
    let Output::Text(hlsl) = translate(&linked, Target::Hlsl(HlslShaderModel::Sm6_0))? else {
        unreachable!("HLSL is text");
    };
    println!("{hlsl}");
    Ok(())
}
