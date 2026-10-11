use std::path::Path;

use shadewire_ir::optimize::optimize;
use shadewire_ir::{BindingOptions, Dag, Error, Location, Reflection, SourceInfo, reflect};
use shadewire_mtlx::document::build::build;
use shadewire_mtlx::include::resolve_includes;
use shadewire_mtlx::library::standard;
use shadewire_mtlx::parse::parse_str;
use shadewire_mtlx::upgrade::upgrade;
use shadewire_mtlx::{ElaborateOptions, elaborate};
use shadewire_wgsl::{EmitOptions, emit};

use crate::options::CompileOptions;

/// The result of a compilation: the generated module, what a host must bind, and the DAG.
#[derive(Clone, Debug)]
pub struct Compiled {
    pub wgsl: String,
    pub reflection: Reflection,
    pub dag: Dag,
}

/// MaterialX text to WGSL: parse, includes, build, upgrade, elaborate, optimize, reflect, emit.
/// `file` names the document (for messages and as the base of relative includes).
pub fn compile_mtlx(
    text: &str,
    file: Option<&Path>,
    material: Option<&str>,
    options: &CompileOptions,
) -> Result<Compiled, Error> {
    let name = file.map(|f| f.display().to_string());
    let root = parse_str(text, name.as_deref())?;
    if file.is_none() {
        if let Some(include) = root.children.iter().find(|c| c.category == "xi:include") {
            let at = Location { file: None, line: include.line, path: String::new() };
            return Err(Error::Parse { at, message: "includes need a file path to resolve against".to_string() });
        }
    }
    let root = resolve_includes(root, file.and_then(Path::parent), name.as_deref())?;
    let document = build(&root, name.as_deref())?;
    let (major, minor) = document.version;
    let document = upgrade(document)?;

    let elaborate_options = ElaborateOptions {
        working_colorspace: options.working_colorspace.clone(),
        values_as_uniforms: options.values_as_uniforms,
    };
    let models: Vec<&str> = options.models.iter().map(String::as_str).collect();
    let mut dag = elaborate(&document, standard(), &elaborate_options, material, &models)?;
    if options.optimize {
        dag = optimize(&dag);
    }

    let bindings = BindingOptions { group: options.group, first_binding: options.first_binding };
    let source = SourceInfo { source: "mtlx".to_string(), mtlx_version: format!("{major}.{minor}") };
    let reflection = reflect(&dag, &bindings, &source);
    let wgsl = emit(&dag, &reflection, &EmitOptions { flip_v: options.flip_v });
    Ok(Compiled { wgsl, reflection, dag })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/materialx/resources/Materials/Examples");

    fn example(relative: &str) -> (String, std::path::PathBuf) {
        let path = Path::new(EXAMPLES).join(relative);
        (std::fs::read_to_string(&path).unwrap(), path)
    }

    #[test]
    fn standard_surface_example_compiles() {
        let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
        let out = compile_mtlx(&text, Some(&path), None, &CompileOptions::default()).unwrap();
        assert_eq!(out.reflection.model, "standard_surface");
        assert_eq!(out.reflection.source, "mtlx");
        assert_eq!(out.reflection.group, 2);
        assert!(out.wgsl.contains("fn sw_eval_surface() -> StandardSurface"), "{}", out.wgsl);
    }

    #[test]
    fn constants_option_removes_the_uniform_block() {
        let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
        let options = CompileOptions { values_as_uniforms: false, ..CompileOptions::default() };
        let out = compile_mtlx(&text, Some(&path), None, &options).unwrap();
        assert!(out.reflection.uniforms.is_empty());
        assert_eq!(out.reflection.uniform_binding, None);
    }

    #[test]
    fn a_model_the_host_refuses_is_no_translation() {
        let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
        let options = CompileOptions { models: vec!["gltf_pbr".to_string()], ..CompileOptions::default() };
        let err = compile_mtlx(&text, Some(&path), None, &options).unwrap_err();
        assert!(matches!(err, Error::NoTranslation { .. }), "{err:?}");
    }

    #[test]
    fn includes_without_a_file_are_an_error() {
        let text = r#"<materialx version="1.39"><xi:include href="other.mtlx"/></materialx>"#;
        let err = compile_mtlx(text, None, None, &CompileOptions::default()).unwrap_err();
        assert!(err.to_string().contains("includes need a file path"), "{err}");
    }

    #[test]
    fn the_document_version_is_reported_as_authored() {
        let text = r#"<materialx version="1.38">
            <standard_surface name="s" type="surfaceshader"/>
            <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>
        </materialx>"#;
        let out = compile_mtlx(text, None, None, &CompileOptions::default()).unwrap();
        assert_eq!(out.reflection.mtlx_version, "1.38");
    }
}
