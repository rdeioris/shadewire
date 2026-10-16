use std::path::{Path, PathBuf};

use shadewire::optimize::optimize;
use shadewire::shader::{CompileOptions, Compiled};
use shadewire::{Error, LOBE_MODELS};
use shadewire_mtlx::{Resolver, WriteOptions, from_ir, material_names, to_ir};
use shadewire_nodes::{IrOptions, MaterialIr};

use crate::args::{Args, Command, CompileArgs, Format, IrArgs, Lang, ListArgs, MtlxArgs, Transparency};

pub fn run(args: Args) -> Result<(), Error> {
    match args.command {
        Command::Compile(compile) => compile_command(&compile),
        Command::Ir(ir) => ir_command(&ir),
        Command::List(list) => list_command(&list),
        Command::Mtlx(mtlx) => mtlx_command(&mtlx),
    }
}

fn io(e: std::io::Error, what: &Path) -> Error {
    Error::Io(format!("{}: {e}", what.display()))
}

/// The format of `file`: `--format`, else by extension.
fn format_of(file: &Path, given: Option<Format>) -> Format {
    given.unwrap_or_else(|| match file.extension().and_then(|e| e.to_str()) {
        Some(e) if e.eq_ignore_ascii_case("json") => Format::Blender,
        Some(e) if e.eq_ignore_ascii_case("mdl") => Format::Mdl,
        _ => Format::Mtlx,
    })
}

/// One material of `file` as IR, through the front end of its format; the front end's warnings go to stderr.
fn read_ir(
    file: &Path,
    format: Option<Format>,
    mdl_paths: &[PathBuf],
    material: Option<&str>,
    options: &IrOptions,
) -> Result<MaterialIr, Error> {
    let text = std::fs::read_to_string(file).map_err(|e| io(e, file))?;
    let location = file.display().to_string();
    let ir = match format_of(file, format) {
        Format::Mtlx => to_ir(&text, Some(&location), material, options, &Resolver::default())?,
        Format::Blender => shadewire_blender::to_ir(&text, material, options)?,
        Format::Mdl => {
            let resolver = shadewire_mdl::Resolver::new(mdl_paths.to_vec());
            shadewire_mdl::to_ir(&text, Some(&location), material, options, &resolver)?
        }
    };
    for warning in &ir.warnings {
        eprintln!("{warning}");
    }
    Ok(ir)
}

/// The material names of `file`.
fn names(file: &Path, format: Option<Format>, mdl_paths: &[PathBuf]) -> Result<Vec<String>, Error> {
    let text = std::fs::read_to_string(file).map_err(|e| io(e, file))?;
    let location = file.display().to_string();
    match format_of(file, format) {
        Format::Mtlx => material_names(&text, Some(&location), &Resolver::default()),
        Format::Blender => shadewire_blender::material_names(&text),
        Format::Mdl => {
            let resolver = shadewire_mdl::Resolver::new(mdl_paths.to_vec());
            shadewire_mdl::material_names(&text, Some(&location), &resolver)
        }
    }
}

/// The front end's options. The models: `--model`'s, or with `--lobes` only the closure tree (and unlit), or else
/// `default` when given, else the front end's own.
fn ir_options(models: &[String], lobes: bool, default: Option<&[&str]>, constants: bool) -> IrOptions {
    let mut options = IrOptions::default();
    let fixed = if lobes { Some(&LOBE_MODELS[..]) } else { default };
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
    let lower = ir_options(&args.models, args.lobes, None, args.constants);
    let ir = read_ir(&args.file, args.format, &args.mdl_paths, args.material.as_deref(), &lower)?;
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
    for name in names(&args.file, args.format, &args.mdl_paths)? {
        println!("{name}");
    }
    Ok(())
}

fn mtlx_command(args: &MtlxArgs) -> Result<(), Error> {
    let lower = ir_options(&args.models, args.lobes, None, args.constants);
    let ir = read_ir(&args.file, args.format, &args.mdl_paths, args.material.as_deref(), &lower)?;
    let dag = if args.optimize { optimize(&ir.dag) } else { ir.dag };
    // `read_ir` without `--material` succeeded, so the document has exactly one material.
    let material = match &args.material {
        Some(material) => material.clone(),
        None => names(&args.file, args.format, &args.mdl_paths)?.swap_remove(0),
    };
    let options = WriteOptions { inline: args.inline, name: material, ..WriteOptions::default() };
    let written = from_ir(&dag, &options)?;
    for warning in &written.warnings {
        eprintln!("{warning}");
    }
    match &args.output {
        Some(path) => std::fs::write(path, &written.text).map_err(|e| io(e, path))?,
        None => print!("{}", written.text),
    }
    Ok(())
}

/// Compiles with the back end of `--lang`.
fn compile(ir: &MaterialIr, args: &CompileArgs) -> Result<Compiled, Error> {
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
        Lang::Mdl => {
            let name = args.material.clone().unwrap_or_else(|| "material".to_string());
            let mdl = shadewire_mdl::MdlOptions { name, optimize: !args.no_optimize };
            shadewire_mdl::compile(dag, source, &mdl)
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
    // The Godot back end has its own model list: the six it maps, and closure trees; MDL takes closure trees.
    let default = match args.lang {
        Lang::Gdshader => Some(&shadewire_gdshader::MODELS[..]),
        Lang::Mdl => Some(&LOBE_MODELS[..]),
        _ => None,
    };
    let lower = ir_options(&args.models, args.lobes, default, args.constants);
    let ir = read_ir(&args.file, args.format, &args.mdl_paths, args.material.as_deref(), &lower)?;
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
