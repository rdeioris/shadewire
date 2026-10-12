use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "shadewire", version, about = "Compile MaterialX materials to shader code")]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Compile a MaterialX document
    Compile(CompileArgs),
    /// Print the IR of a MaterialX material (the DAG the shader is emitted from)
    Ir(IrArgs),
}

#[derive(Copy, Clone, Debug, PartialEq, ValueEnum)]
pub enum Lang {
    Wgsl,
    Hlsl,
    Glsl,
    GlslEs,
    Msl,
    Spirv,
}

#[derive(Parser, Debug)]
pub struct IrArgs {
    /// The .mtlx document
    pub file: PathBuf,
    /// Material name, when the document has several
    #[arg(long)]
    pub material: Option<String>,
    /// Surface models the host accepts (repeatable); default: all four
    #[arg(long = "model")]
    pub models: Vec<String>,
    /// Print the IR as elaborated, before the optimizer
    #[arg(long)]
    pub no_optimize: bool,
    /// Bake authored values in as constants instead of uniforms
    #[arg(long)]
    pub constants: bool,
}

#[derive(Parser, Debug)]
pub struct CompileArgs {
    /// The .mtlx document
    pub file: PathBuf,
    /// Material name, when the document has several
    #[arg(long)]
    pub material: Option<String>,
    /// Surface models the host accepts (repeatable); default: all four
    #[arg(long = "model")]
    pub models: Vec<String>,
    #[arg(long, value_enum, default_value = "wgsl")]
    pub lang: Lang,
    /// Write the reflection JSON here
    #[arg(long)]
    pub reflection: Option<PathBuf>,
    /// Write the shader here instead of stdout
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,
    /// Skip the optimizer
    #[arg(long)]
    pub no_optimize: bool,
    /// Bake authored values in as constants instead of uniforms
    #[arg(long)]
    pub constants: bool,
    /// Keep binding 0 for the uniform block even without uniforms, so textures start at 1
    #[arg(long)]
    pub reserve_uniform_binding: bool,
}
