//! MaterialX through the WGSL back end.

mod common;

use std::path::Path;

use common::compile_mtlx;
use shadewire::Error;
use shadewire_mtlx::{MtlxOptions, to_ir};
use shadewire_wgsl::{CompileOptions, compile};

const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/materialx/resources/Materials/Examples");

fn example(relative: &str) -> (String, std::path::PathBuf) {
    let path = Path::new(EXAMPLES).join(relative);
    (std::fs::read_to_string(&path).unwrap(), path)
}

#[test]
fn compile_without_the_optimizer_keeps_the_raw_dag() {
    let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
    let ir = to_ir(&text, Some(&path), None, &MtlxOptions::default()).unwrap();
    let raw = CompileOptions { optimize: false, ..CompileOptions::default() };
    assert_eq!(compile(&ir.dag, &ir.source, &raw).unwrap().dag.nodes.len(), ir.dag.nodes.len());
    let optimized = compile(&ir.dag, &ir.source, &CompileOptions::default()).unwrap();
    assert!(optimized.dag.nodes.len() < ir.dag.nodes.len());
}

#[test]
fn standard_surface_example_compiles() {
    let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
    let out = compile_mtlx(&text, Some(&path), None, &MtlxOptions::default(), &CompileOptions::default()).unwrap();
    assert_eq!(out.reflection.model, "standard_surface");
    assert_eq!(out.reflection.source, "mtlx");
    assert_eq!(out.reflection.group, 2);
    assert!(out.wgsl.contains("fn shadewire_eval_surface() -> StandardSurface"), "{}", out.wgsl);
}

/// Hextile gradients use derivatives: hosts learn that the function is fragment-only.
#[test]
fn derivatives_are_reported_fragment_only() {
    let (text, path) = example("StandardSurface/standard_surface_onyx_hextiled.mtlx");
    let out = compile_mtlx(&text, Some(&path), None, &MtlxOptions::default(), &CompileOptions::default()).unwrap();
    assert!(out.reflection.approximations.contains(&"fragment stage only (derivatives)".to_string()));
    let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
    let out = compile_mtlx(&text, Some(&path), None, &MtlxOptions::default(), &CompileOptions::default()).unwrap();
    assert!(out.reflection.approximations.is_empty(), "{:?}", out.reflection.approximations);
}

#[test]
fn constants_option_removes_the_uniform_block() {
    let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
    let mtlx = MtlxOptions { values_as_uniforms: false, ..MtlxOptions::default() };
    let out = compile_mtlx(&text, Some(&path), None, &mtlx, &CompileOptions::default()).unwrap();
    assert!(out.reflection.uniforms.is_empty());
    assert_eq!(out.reflection.uniform_binding, None);
}

#[test]
fn a_model_the_host_refuses_is_no_translation() {
    let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
    let mtlx = MtlxOptions { models: vec!["gltf_pbr".to_string()], ..MtlxOptions::default() };
    let err = compile_mtlx(&text, Some(&path), None, &mtlx, &CompileOptions::default()).unwrap_err();
    assert!(matches!(err, Error::NoTranslation { .. }), "{err:?}");
}

/// An srgb color converts to the working space in the shader: the field
/// is computed from its uniform, so the host cannot pass it through.
#[test]
fn a_converted_uniform_is_a_uniform_expression() {
    let text = r#"<materialx version="1.39">
        <standard_surface name="s" type="surfaceshader">
          <input name="base_color" type="color3" value="0.5, 0.5, 0.5" colorspace="srgb_texture"/>
          <input name="base" type="float" value="0.5"/>
        </standard_surface>
        <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>
    </materialx>"#;
    let out = compile_mtlx(text, None, None, &MtlxOptions::default(), &CompileOptions::default()).unwrap();
    let color = &out.reflection.params["base_color"];
    assert_eq!((color.kind, color.uniform.as_deref()), ("uniform_expr", None));
    assert!(out.reflection.uniforms.iter().any(|u| u.name == "base_color"));
    let base = &out.reflection.params["base"];
    assert_eq!((base.kind, base.uniform.as_deref()), ("uniform", Some("base")));
}

/// One host function per reflection name: `shadewire_host_texcoord_0` cannot return two types.
#[test]
fn one_context_read_at_two_types_is_an_error() {
    let text = r#"<materialx version="1.39">
        <texcoord name="uv2" type="vector2"/>
        <texcoord name="uv3" type="vector3"/>
        <extract name="u" type="float"><input name="in" type="vector2" nodename="uv2"/><input name="index" type="integer" value="0"/></extract>
        <extract name="w" type="float"><input name="in" type="vector3" nodename="uv3"/><input name="index" type="integer" value="2"/></extract>
        <surface_unlit name="s" type="surfaceshader">
          <input name="emission" type="float" nodename="u"/>
          <input name="transmission" type="float" nodename="w"/>
        </surface_unlit>
        <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>
    </materialx>"#;
    let err = compile_mtlx(text, None, None, &MtlxOptions::default(), &CompileOptions::default()).unwrap_err();
    match err {
        Error::TypeMismatch { at, expected, found } => {
            assert_eq!((at.path.as_str(), expected.as_str(), found.as_str()), ("uv3", "vector2", "vector3"));
        }
        other => panic!("expected a type mismatch, got {other:?}"),
    }
}

#[test]
fn the_document_version_is_reported_as_authored() {
    let text = r#"<materialx version="1.38">
        <standard_surface name="s" type="surfaceshader"/>
        <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>
    </materialx>"#;
    let out = compile_mtlx(text, None, None, &MtlxOptions::default(), &CompileOptions::default()).unwrap();
    assert_eq!(out.reflection.mtlx_version, "1.38");
}
