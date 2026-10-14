use shadewire::Error;
use shadewire::optimize::optimize;
use shadewire::shader::{CompileOptions, Compiled};
use shadewire_mtlx::{Resolver, material_names, to_ir};
use shadewire_nodes::LowerOptions;

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

/// The front end's options. The models: `--model`'s, or with `--lobes` only the closure tree (and unlit), or else
/// `default` when given, else the front end's own.
fn lower_options(models: &[String], lobes: bool, default: Option<&[&str]>, constants: bool) -> LowerOptions {
    let mut options = LowerOptions::default();
    let fixed = if lobes { Some(&shadewire_gdshader::LOBE_MODELS[..]) } else { default };
    if !models.is_empty() {
        options.models = models.to_vec();
    } else if let Some(fixed) = fixed {
        options.models = fixed.iter().map(|m| m.to_string()).collect();
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
    let lower = lower_options(&args.models, args.lobes, None, args.constants);
    let ir =
        to_ir(&text, Some(&args.file.display().to_string()), args.material.as_deref(), &lower, &Resolver::default())?;
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
    for name in material_names(&text, Some(&args.file.display().to_string()), &Resolver::default())? {
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
        Lang::Glsl | Lang::GlslEs => {
            let version = shadewire_glsl::Version::new(args.lang == Lang::GlslEs, args.glsl_version);
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
    let transparency = match args.transparency.unwrap_or(Transparency::Auto) {
        Transparency::Auto => T::Auto,
        Transparency::Opaque => T::Opaque,
        Transparency::Scissor => T::Scissor,
        Transparency::Hash => T::Hash,
        Transparency::Blend => T::Blend,
    };
    shadewire_gdshader::GdshaderOptions {
        optimize: !args.no_optimize,
        transparency,
        emission_scale: args.emission_scale.unwrap_or(1.0),
        ..shadewire_gdshader::GdshaderOptions::default()
    }
}

fn compile_command(args: &CompileArgs) -> Result<(), Error> {
    let text = std::fs::read_to_string(&args.file).map_err(|e| io(e, &args.file))?;
    // The Godot back end has its own model list: the six it maps, and closure trees.
    let default = (args.lang == Lang::Gdshader).then_some(&shadewire_gdshader::MODELS[..]);
    let lower = lower_options(&args.models, args.lobes, default, args.constants);
    let ir =
        to_ir(&text, Some(&args.file.display().to_string()), args.material.as_deref(), &lower, &Resolver::default())?;
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
