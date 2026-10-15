//! MaterialX through the WGSL back end.

mod common;

use std::path::Path;

use common::compile_mtlx;
use shadewire::{ContextRead, Error, NodeKind, Op, Space};
use shadewire_mtlx::{Resolver, to_ir};
use shadewire_nodes::IrOptions;
use shadewire_wgsl::{CompileOptions, compile};

const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/materialx/resources/Materials/Examples");

fn example(relative: &str) -> (String, std::path::PathBuf) {
    let path = Path::new(EXAMPLES).join(relative);
    (std::fs::read_to_string(&path).unwrap(), path)
}

#[test]
fn compile_without_the_optimizer_keeps_the_raw_dag() {
    let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
    let ir = to_ir(&text, path.to_str(), None, &IrOptions::default(), &Resolver::default()).unwrap();
    let raw = CompileOptions { optimize: false, ..CompileOptions::default() };
    assert_eq!(compile(&ir.dag, &ir.source, &raw).unwrap().dag.nodes.len(), ir.dag.nodes.len());
    let optimized = compile(&ir.dag, &ir.source, &CompileOptions::default()).unwrap();
    assert!(optimized.dag.nodes.len() < ir.dag.nodes.len());
}

#[test]
fn standard_surface_example_compiles() {
    let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
    let out = compile_mtlx(&text, Some(&path), None, &IrOptions::default(), &CompileOptions::default()).unwrap();
    assert_eq!(out.reflection.model, "standard_surface");
    assert_eq!(out.reflection.source, "mtlx");
    assert_eq!(out.reflection.group, 2);
    assert!(out.code.contains("fn shadewire_eval_surface() -> StandardSurface"), "{}", out.code);
}

/// Hextile gradients use derivatives: hosts learn that the function is fragment-only.
#[test]
fn derivatives_are_reported_fragment_only() {
    let (text, path) = example("StandardSurface/standard_surface_onyx_hextiled.mtlx");
    let out = compile_mtlx(&text, Some(&path), None, &IrOptions::default(), &CompileOptions::default()).unwrap();
    assert!(out.reflection.approximations.contains(&"fragment stage only (derivatives)".to_string()));
    let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
    let out = compile_mtlx(&text, Some(&path), None, &IrOptions::default(), &CompileOptions::default()).unwrap();
    assert!(out.reflection.approximations.is_empty(), "{:?}", out.reflection.approximations);
}

#[test]
fn constants_option_removes_the_uniform_block() {
    let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
    let lower = IrOptions { values_as_uniforms: false, ..IrOptions::default() };
    let out = compile_mtlx(&text, Some(&path), None, &lower, &CompileOptions::default()).unwrap();
    assert!(out.reflection.uniforms.is_empty());
    assert_eq!(out.reflection.uniform_binding, None);
}

#[test]
fn a_model_the_host_refuses_is_no_translation() {
    let (text, path) = example("StandardSurface/standard_surface_default.mtlx");
    let lower = IrOptions::with_models(&["gltf_pbr"]);
    let err = compile_mtlx(&text, Some(&path), None, &lower, &CompileOptions::default()).unwrap_err();
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
    let out = compile_mtlx(text, None, None, &IrOptions::default(), &CompileOptions::default()).unwrap();
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
    let err = compile_mtlx(text, None, None, &IrOptions::default(), &CompileOptions::default()).unwrap_err();
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
    let out = compile_mtlx(text, None, None, &IrOptions::default(), &CompileOptions::default()).unwrap();
    assert_eq!(out.reflection.mtlx_version, "1.38");
}

/// A document of `version` with `nodes` and a `surface_unlit` whose `input` (type `ty`) reads the node `out`.
fn unlit(version: &str, nodes: &str, input: &str, ty: &str) -> String {
    format!(
        r#"<materialx version="{version}">
        {nodes}
        <surface_unlit name="s" type="surfaceshader"><input name="{input}" type="{ty}" nodename="out"/></surface_unlit>
        <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>
    </materialx>"#
    )
}

/// The constant the optimizer folds the unlit `input` to, with values baked in.
fn folded(text: &str, input: &str) -> Vec<f32> {
    let ir = IrOptions { values_as_uniforms: false, ..IrOptions::default() };
    let out = compile_mtlx(text, None, None, &ir, &CompileOptions::default()).unwrap_or_else(|e| panic!("{e}"));
    let field = out.dag.surface.fields.iter().find(|f| f.name == input).unwrap();
    match &out.dag.node(field.node).kind {
        NodeKind::Const(v) => v.floats(),
        other => panic!("{input} is not a constant: {other:?}"),
    }
}

fn close(ours: &[f32], want: &[f32]) -> bool {
    ours.len() == want.len() && ours.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-5)
}

/// plus, minus, difference, screen, burn and dodge follow upstream's genglsl, `mix * blend + (1 - mix) * bg`,
/// for float, color3 and color4 (whose alpha blends like the other channels).
#[test]
fn compositing_nodes_follow_their_genglsl() {
    let cases: [(&str, &str, [f32; 3]); 7] = [
        ("plus", "", [0.8, 0.6, 1.2]),
        ("minus", "", [0.4, -0.4, -0.6]),
        ("difference", "", [0.4, 0.4, 0.6]),
        ("difference", r#"<input name="mix" type="float" value="0.5"/>"#, [0.5, 0.25, 0.45]),
        ("screen", "", [0.68, 0.55, 0.93]),
        ("burn", "", [-1.0, -0.8, 1.0 - 0.7 / 0.9]),
        ("dodge", "", [0.75, 0.2, 3.0]),
    ];
    for (category, mix, want) in cases {
        let color3 = format!(
            r#"<{category} name="out" type="color3"><input name="fg" type="color3" value="0.2, 0.5, 0.9"/><input name="bg" type="color3" value="0.6, 0.1, 0.3"/>{mix}</{category}>"#
        );
        let ours = folded(&unlit("1.39", &color3, "emission_color", "color3"), "emission_color");
        assert!(close(&ours, &want), "{category} color3 {mix}: {ours:?}, want {want:?}");
        // The green channel's fg and bg, as a float and as a color4's alpha.
        let float = format!(
            r#"<{category} name="out" type="float"><input name="fg" type="float" value="0.5"/><input name="bg" type="float" value="0.1"/>{mix}</{category}>"#
        );
        let ours = folded(&unlit("1.39", &float, "emission", "float"), "emission");
        assert!(close(&ours, &want[1..2]), "{category} float {mix}: {ours:?}, want {}", want[1]);
        let color4 = format!(
            r#"<{category} name="c" type="color4"><input name="fg" type="color4" value="0.5, 0.2, 0.9, 0.5"/><input name="bg" type="color4" value="0.1, 0.6, 0.3, 0.1"/>{mix}</{category}>
               <extract name="out" type="float"><input name="in" type="color4" nodename="c"/><input name="index" type="integer" value="3"/></extract>"#
        );
        let ours = folded(&unlit("1.39", &color4, "emission", "float"), "emission");
        assert!(close(&ours, &want[1..2]), "{category} color4 alpha {mix}: {ours:?}, want {}", want[1]);
    }
}

/// mx_burn_float and mx_dodge_float return 0, unmixed, where their divisor is under M_FLOAT_EPS.
#[test]
fn burn_and_dodge_guard_their_divisor() {
    for (category, fg) in [("burn", "0.0"), ("dodge", "1.0")] {
        let node = format!(
            r#"<{category} name="out" type="float"><input name="fg" type="float" value="{fg}"/><input name="bg" type="float" value="0.6"/><input name="mix" type="float" value="0.5"/></{category}>"#
        );
        assert_eq!(folded(&unlit("1.39", &node, "emission", "float"), "emission"), [0.0], "{category}");
    }
}

#[test]
fn fractal2d_calls_the_2d_fbm_for_every_width() {
    for (ty, call) in [
        ("float", "shadewire_fractal_noise_float_2d"),
        ("vector2", "shadewire_fractal_noise_vec2_2d"),
        ("color3", "shadewire_fractal_noise_vec3_2d"),
        ("color4", "shadewire_fractal_noise_vec4_2d"),
    ] {
        // No convert from color3 to itself: a dot passes it through.
        let to_color3 = if ty == "color3" { "dot" } else { "convert" };
        let nodes = format!(
            r#"<texcoord name="uv" type="vector2"/>
               <fractal2d name="f" type="{ty}"><input name="texcoord" type="vector2" nodename="uv"/></fractal2d>
               <{to_color3} name="out" type="color3"><input name="in" type="{ty}" nodename="f"/></{to_color3}>"#
        );
        let text = unlit("1.39", &nodes, "emission_color", "color3");
        let out = compile_mtlx(&text, None, None, &IrOptions::default(), &CompileOptions::default())
            .unwrap_or_else(|e| panic!("{ty}: {e}"));
        assert!(out.dag.nodes.iter().any(|n| matches!(n.kind, NodeKind::Op { op: Op::Fractal2d, .. })), "{ty}");
        assert!(out.code.contains(&format!("{call}(")), "{ty}: {}", out.code);
    }
}

/// 1.39's libraries ship no viewdirection nodedef; shadewire's reads the host's view direction, in world space
/// unless `space` says otherwise.
#[test]
fn viewdirection_reads_the_host_in_its_space() {
    for (space, want) in [("", Space::World), (r#"<input name="space" type="string" value="object"/>"#, Space::Object)]
    {
        let nodes = format!(
            r#"<viewdirection name="v" type="vector3">{space}</viewdirection>
               <convert name="out" type="color3"><input name="in" type="vector3" nodename="v"/></convert>"#
        );
        let text = unlit("1.39", &nodes, "emission_color", "color3");
        let out = compile_mtlx(&text, None, None, &IrOptions::default(), &CompileOptions::default()).unwrap();
        let reads: Vec<&ContextRead> = out
            .dag
            .nodes
            .iter()
            .filter_map(|n| if let NodeKind::Context(r) = &n.kind { Some(r) } else { None })
            .collect();
        assert_eq!(reads, [&ContextRead::ViewDirection(want)], "{space}");
    }
}

/// A 1.38 object-space normalmap upgrades to `normalize(in * 2 - 1)`, as upstream's upgrade writes it. Mid-grey
/// decodes to a zero vector, whose normalize is left to the GPU: no JSON or shader can hold the NaN it folds to.
#[test]
fn object_space_normalmap_upgrades_to_a_decode() {
    let nodes = r#"<normalmap name="n" type="vector3">
          <input name="in" type="vector3" value="1.0, 0.5, 0.5"/><input name="space" type="string" value="object"/>
          <input name="scale" type="float" value="0.25"/>
        </normalmap>
        <convert name="out" type="color3"><input name="in" type="vector3" nodename="n"/></convert>"#;
    let ours = folded(&unlit("1.38", nodes, "emission_color", "color3"), "emission_color");
    assert!(close(&ours, &[1.0, 0.0, 0.0]), "{ours:?}");

    let nodes = r#"<normalmap name="n" type="vector3">
          <input name="in" type="vector3" value="0.5, 0.5, 0.5"/><input name="space" type="string" value="object"/>
        </normalmap>
        <convert name="out" type="color3"><input name="in" type="vector3" nodename="n"/></convert>"#;
    let text = unlit("1.38", nodes, "emission_color", "color3");
    let ir = IrOptions { values_as_uniforms: false, ..IrOptions::default() };
    let out = compile_mtlx(&text, None, None, &ir, &CompileOptions::default()).unwrap();
    assert!(out.dag.nodes.iter().any(|n| matches!(n.kind, NodeKind::Op { op: Op::Normalize, .. })));
    let json = serde_json::to_string(&shadewire::export::export(&out.dag)).unwrap();
    assert!(!json.contains("null"), "{json}");
}
