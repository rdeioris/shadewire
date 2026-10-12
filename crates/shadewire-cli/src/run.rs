use shadewire::{
    CompileOptions, Compiled, Error, GlslVersion, HlslShaderModel, MslVersion, Output, Target, compile_mtlx, link,
    model_struct_name, reference_host, translate,
};

use clap::ValueEnum;

use crate::args::{Args, Command, CompileArgs, Lang};

pub fn run(args: Args) -> Result<(), Error> {
    match args.command {
        Command::Compile(compile) => compile_command(&compile),
    }
}

fn io(e: std::io::Error, what: &std::path::Path) -> Error {
    Error::Io(format!("{}: {e}", what.display()))
}

fn options(args: &CompileArgs) -> CompileOptions {
    let mut options = CompileOptions::default();
    if !args.models.is_empty() {
        options.models = args.models.clone();
    }
    options.optimize = !args.no_optimize;
    options.values_as_uniforms = !args.constants;
    options
}

fn target(lang: Lang) -> Target {
    match lang {
        Lang::Wgsl => Target::Wgsl,
        Lang::Hlsl => Target::Hlsl(HlslShaderModel::Sm6_0),
        Lang::Glsl => Target::Glsl(GlslVersion::Desktop(450)),
        Lang::GlslEs => Target::Glsl(GlslVersion::Es(300)),
        Lang::Msl => Target::Msl(MslVersion { major: 2, minor: 1 }),
        Lang::Spirv => Target::SpirV,
    }
}

/// The language as typed after `--lang`, such as `glsl-es`.
fn lang_name(lang: Lang) -> String {
    lang.to_possible_value().map_or_else(String::new, |v| v.get_name().to_string())
}

/// The text or bytes to write: generated WGSL as is, or the linked module translated.
fn render(compiled: &Compiled, lang: Lang) -> Result<Vec<u8>, Error> {
    if lang == Lang::Wgsl {
        return Ok(compiled.wgsl.clone().into_bytes());
    }
    eprintln!("note: {} output links the reference host, which supplies the entry point", lang_name(lang));
    let prelude = reference_host::prelude(&compiled.reflection, &model_struct_name(&compiled.reflection.model));
    let linked = link(&compiled.wgsl, &prelude)?;
    Ok(match translate(&linked, target(lang))? {
        Output::Text(text) => text.into_bytes(),
        Output::Words(words) => words.iter().flat_map(|w| w.to_le_bytes()).collect(),
    })
}

fn compile_command(args: &CompileArgs) -> Result<(), Error> {
    let text = std::fs::read_to_string(&args.file).map_err(|e| io(e, &args.file))?;
    let compiled = compile_mtlx(&text, Some(&args.file), args.material.as_deref(), &options(args))?;
    let bytes = render(&compiled, args.lang)?;
    if args.output.is_none() && args.lang == Lang::Spirv {
        return Err(Error::Io("SPIR-V is binary: give -o".to_string()));
    }
    // Only a shader that rendered gets a reflection file, so the two always match.
    if let Some(path) = &args.reflection {
        let json = serde_json::to_string_pretty(&compiled.reflection).map_err(|e| Error::Io(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| io(e, path))?;
    }
    match &args.output {
        Some(path) => std::fs::write(path, bytes).map_err(|e| io(e, path))?,
        None => print!("{}", String::from_utf8_lossy(&bytes)),
    }
    Ok(())
}
