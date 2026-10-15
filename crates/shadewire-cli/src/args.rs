use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "shadewire",
    version,
    about = "Compile MaterialX documents and Blender material graphs to shader code"
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Compile a MaterialX document or a Blender graph
    Compile(CompileArgs),
    /// Print the IR of a material (the DAG the shader is emitted from)
    Ir(IrArgs),
    /// List the materials of a document, one name per line
    List(ListArgs),
    /// Write a material back as a MaterialX document (library nodes kept as nodes)
    Mtlx(MtlxArgs),
}

#[derive(Parser, Debug)]
pub struct ListArgs {
    /// The .mtlx document or .json Blender graph
    pub file: PathBuf,
    /// The input's format; default: by extension (`.json` is a Blender graph, anything else MaterialX)
    #[arg(long, value_enum)]
    pub format: Option<Format>,
}

#[derive(Parser, Debug)]
pub struct MtlxArgs {
    /// The .mtlx document or .json Blender graph
    pub file: PathBuf,
    /// The input's format; default: by extension (`.json` is a Blender graph, anything else MaterialX)
    #[arg(long, value_enum)]
    pub format: Option<Format>,
    /// Material name, when the document has several
    #[arg(long)]
    pub material: Option<String>,
    /// Surface models the host accepts (repeatable); default: the four most hosts map
    #[arg(long = "model")]
    pub models: Vec<String>,
    /// Open every material into its closure tree (accept only `surface` and `surface_unlit`)
    #[arg(long, conflicts_with = "models")]
    pub lobes: bool,
    /// Write every library node expanded to leaf ops instead of as one node
    #[arg(long)]
    pub inline: bool,
    /// Run the optimizer first (folds constants; expands library nodes)
    #[arg(long)]
    pub optimize: bool,
    /// Bake authored values in as constants instead of uniforms
    #[arg(long)]
    pub constants: bool,
    /// Write the document here instead of stdout
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,
}

/// The format of an input document.
#[derive(Copy, Clone, Debug, PartialEq, ValueEnum)]
pub enum Format {
    /// A MaterialX document
    Mtlx,
    /// A Blender material graph (the Blender extension's File > Export > Material Graph)
    Blender,
}

/// The language of the generated code: each a back end of its own.
#[derive(Copy, Clone, Debug, PartialEq, ValueEnum)]
pub enum Lang {
    Wgsl,
    Glsl,
    GlslEs,
    Hlsl,
    Msl,
    /// A complete Godot 4 spatial shader
    Gdshader,
}

/// How opacity reaches Godot (`--lang gdshader`).
#[derive(Copy, Clone, Debug, PartialEq, ValueEnum)]
pub enum Transparency {
    Auto,
    Opaque,
    Scissor,
    Hash,
    Blend,
}

#[derive(Parser, Debug)]
pub struct IrArgs {
    /// The .mtlx document or .json Blender graph
    pub file: PathBuf,
    /// The input's format; default: by extension (`.json` is a Blender graph, anything else MaterialX)
    #[arg(long, value_enum)]
    pub format: Option<Format>,
    /// Material name, when the document has several
    #[arg(long)]
    pub material: Option<String>,
    /// Surface models the host accepts (repeatable); default: the four most hosts map
    #[arg(long = "model")]
    pub models: Vec<String>,
    /// Open every material into its closure tree (accept only `surface` and `surface_unlit`)
    #[arg(long, conflicts_with = "models")]
    pub lobes: bool,
    /// Print the IR as elaborated, before the optimizer
    #[arg(long)]
    pub no_optimize: bool,
    /// Bake authored values in as constants instead of uniforms
    #[arg(long)]
    pub constants: bool,
    /// Print the IR as a JSON document (`shadewire::export`) instead of text
    #[arg(long)]
    pub json: bool,
}

#[derive(Parser, Debug)]
pub struct CompileArgs {
    /// The .mtlx document or .json Blender graph
    pub file: PathBuf,
    /// The input's format; default: by extension (`.json` is a Blender graph, anything else MaterialX)
    #[arg(long, value_enum)]
    pub format: Option<Format>,
    /// Material name, when the document has several
    #[arg(long)]
    pub material: Option<String>,
    /// Surface models the host accepts (repeatable); default: the four most hosts map, and for gdshader its own
    /// list (`shadewire_gdshader::MODELS`)
    #[arg(long = "model")]
    pub models: Vec<String>,
    /// Open every material into its closure tree (accept only `surface` and `surface_unlit`) and write
    /// MaterialX's reference lobes to light it; gdshader calls them from a light() function
    #[arg(long, conflicts_with = "models")]
    pub lobes: bool,
    #[arg(long, value_enum, default_value = "wgsl")]
    pub lang: Lang,
    /// The GLSL version: default 420 for `glsl`, 300 for `glsl-es`
    #[arg(long)]
    pub glsl_version: Option<u16>,
    /// Write the reflection JSON here
    #[arg(long)]
    pub reflection: Option<PathBuf>,
    /// Write the code here instead of stdout
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
    /// gdshader: how opacity reaches Godot [default: auto]
    #[arg(long, value_enum)]
    pub transparency: Option<Transparency>,
    /// gdshader: multiplies EMISSION [default: 1]
    #[arg(long)]
    pub emission_scale: Option<f32>,
}

impl CompileArgs {
    /// The first flag given that only `--lang gdshader` reads, when the language is another.
    pub fn misplaced_gdshader_flag(&self) -> Option<&'static str> {
        if self.lang == Lang::Gdshader {
            None
        } else if self.transparency.is_some() {
            Some("--transparency")
        } else if self.emission_scale.is_some() {
            Some("--emission-scale")
        } else {
            None
        }
    }
}
