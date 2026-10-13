use shadewire::Error;
use shadewire::optimize::optimize;
use shadewire::shader::{CompileOptions, Compiled};
use shadewire_mtlx::{MtlxOptions, material_names, to_ir};

use crate::args::{Args, Command, CompileArgs, IrArgs, Lang, ListArgs, Transparency};

pub fn run(args: Args) -> Result<(), Error> {
    match args.command {
        Command::Compile(compile) => compile_command(&compile),
        Command::Ir(ir) => ir_command(&ir),
        Command::List(list) => list_command(&list),
    }
}

fn io(e: std::io::Error, what: &std::path::Path) -> Error {
    Error::Io(format!("{}: {e}", what.display()))
}

fn mtlx_options(models: &[String], constants: bool) -> MtlxOptions {
    let mut options = MtlxOptions::default();
    if !models.is_empty() {
        options.models = models.to_vec();
    }
    options.values_as_uniforms = !constants;
    options
}

fn compile_options(args: &CompileArgs) -> CompileOptions {
    CompileOptions {
        optimize: !args.no_optimize,
        reserve_uniform_binding: args.reserve_uniform_binding,
        ..CompileOptions::default()
    }
}

fn ir_command(args: &IrArgs) -> Result<(), Error> {
    let text = std::fs::read_to_string(&args.file).map_err(|e| io(e, &args.file))?;
    let mtlx = mtlx_options(&args.models, args.constants);
    let ir = to_ir(&text, Some(&args.file), args.material.as_deref(), &mtlx)?;
    let dag = if args.no_optimize { ir.dag } else { optimize(&ir.dag) };
    if args.json {
        let text = serde_json::to_string(&shadewire::export::export(&dag)).map_err(|e| Error::Io(e.to_string()))?;
        println!("{text}");
    } else {
        print!("{}", dag.dump());
    }
    Ok(())
}

fn list_command(args: &ListArgs) -> Result<(), Error> {
    let text = std::fs::read_to_string(&args.file).map_err(|e| io(e, &args.file))?;
    for name in material_names(&text, Some(&args.file))? {
        println!("{name}");
    }
    Ok(())
}

/// Compiles with the back end of `--lang`.
fn compile(ir: &shadewire_mtlx::MaterialIr, args: &CompileArgs) -> Result<Compiled, Error> {
    let options = compile_options(args);
    let (dag, source) = (&ir.dag, &ir.source);
    match args.lang {
        Lang::Wgsl => shadewire_wgsl::compile(dag, source, &options),
        Lang::Glsl => {
            let version = shadewire_glsl::Version::Desktop(args.glsl_version.unwrap_or(420));
            shadewire_glsl::compile(dag, source, &options, version)
        }
        Lang::GlslEs => {
            let version = shadewire_glsl::Version::Es(args.glsl_version.unwrap_or(300));
            shadewire_glsl::compile(dag, source, &options, version)
        }
        Lang::Hlsl => shadewire_hlsl::compile(dag, source, &options),
        Lang::Msl => shadewire_msl::compile(dag, source, &options),
        Lang::Gdshader => {
            let out = shadewire_gdshader::compile(dag, source, &gdshader_options(args))?;
            Ok(Compiled { code: out.code, reflection: out.reflection, dag: out.dag })
        }
    }
}

fn gdshader_options(args: &CompileArgs) -> shadewire_gdshader::GdshaderOptions {
    use shadewire_gdshader::Transparency as T;
    let transparency = match args.transparency {
        Transparency::Auto => T::Auto,
        Transparency::Opaque => T::Opaque,
        Transparency::Scissor => T::Scissor,
        Transparency::Hash => T::Hash,
        Transparency::Blend => T::Blend,
    };
    shadewire_gdshader::GdshaderOptions {
        optimize: !args.no_optimize,
        transparency,
        emission_scale: args.emission_scale,
    }
}

fn compile_command(args: &CompileArgs) -> Result<(), Error> {
    let text = std::fs::read_to_string(&args.file).map_err(|e| io(e, &args.file))?;
    let mut mtlx = mtlx_options(&args.models, args.constants);
    // The Godot back end has its own model lists: the six it maps (and closure trees), or only closure trees.
    if args.lang == Lang::Gdshader && args.models.is_empty() {
        let models: &[&str] = if args.lobes { &shadewire_gdshader::LOBE_MODELS } else { &shadewire_gdshader::MODELS };
        mtlx.models = models.iter().map(|m| m.to_string()).collect();
    }
    let ir = to_ir(&text, Some(&args.file), args.material.as_deref(), &mtlx)?;
    let compiled = compile(&ir, args)?;
    if let Some(path) = &args.reflection {
        let json = serde_json::to_string_pretty(&compiled.reflection).map_err(|e| Error::Io(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| io(e, path))?;
    }
    match &args.output {
        Some(path) => std::fs::write(path, &compiled.code).map_err(|e| io(e, path))?,
        None => print!("{}", compiled.code),
    }
    Ok(())
}
