//! The WGSL back end over the shared fixtures: every generated module validates in naga, with the stage rules of
//! a fragment shader.

use shadewire::shader::fixtures::{self, add, reflection};
use shadewire::*;

use crate::validate::{check, naga};
use crate::{MaterialOptions, emit};

fn wgsl(dag: &Dag) -> (String, Reflection) {
    let r = reflection(dag);
    (emit(dag, &r, &MaterialOptions::default()), r)
}

#[test]
fn every_fixture_validates() {
    for (name, dag) in fixtures::all() {
        let (text, r) = wgsl(&dag);
        assert!(!text.contains("fn shadewire_host_"), "{name}: host functions are the host's");
        check(&text, &r).unwrap();
    }
}

#[test]
fn without_implicit_derivatives_every_fixture_validates() {
    let options = MaterialOptions { implicit_derivatives: false, ..MaterialOptions::default() };
    for (name, dag) in fixtures::all() {
        let r = reflection(&dag);
        let text = emit(&dag, &r, &options);
        assert!(!text.contains("textureSample(") && !text.contains("dpdx"), "{name}: {text}");
        check(&text, &r).unwrap();
    }
}

#[test]
fn a_small_module_reads_as_written() {
    let mut dag = Dag::new("open_pbr_surface");
    let fg =
        add(&mut dag, NodeKind::Uniform { name: "tint".into(), value: Value::Color3([1.0, 0.5, 0.25]) }, Type::Color3);
    let bg = add(&mut dag, NodeKind::Const(Value::Color3([0.0, 0.0, 1.0])), Type::Color3);
    add(&mut dag, NodeKind::Const(Value::Float(9.0)), Type::Float);
    let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
    let x = add(&mut dag, NodeKind::Op { op: Op::Extract(0), args: vec![uv] }, Type::Float);
    let mix = add(&mut dag, NodeKind::Op { op: Op::Mix, args: vec![fg, bg, x] }, Type::Color3);
    dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: mix });
    dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: x });
    let (text, r) = wgsl(&dag);
    let expected = "\
struct OpenPbrSurface {
    base_color: vec3<f32>,
    base_weight: f32,
}

struct MaterialUniforms {
    tint: vec3<f32>,
}

@group(0) @binding(0) var<uniform> shadewire_u: MaterialUniforms;

fn shadewire_eval_surface() -> OpenPbrSurface {
    let n0: vec3<f32> = shadewire_u.tint;
    let n1: vec3<f32> = vec3<f32>(0.0, 0.0, 1.0);
    let n3: vec2<f32> = shadewire_host_texcoord_0();
    let n4: f32 = n3[0i];
    let n5: vec3<f32> = n1 + (n0 - n1) * n4;
    return OpenPbrSurface(n5, n4);
}
";
    assert_eq!(text, expected);
    check(&text, &r).unwrap();
}

#[test]
fn uniforms_reads_and_samples() {
    let (text, _) = wgsl(&fixtures::everything());
    assert!(text.contains("let n3: bool = shadewire_u.flag != 0u;"), "{text}");
    assert!(text.contains("shadewire_host_geomprop_my_prop()"), "{text}");
    assert!(text.contains("normalize(shadewire_host_transform_normal_object_to_world(n6))"), "{text}");
    assert!(
        text.contains("select(textureSample(shadewire_t1, shadewire_s1, vec2<f32>(n0.x, 1.0 - n0.y)).x, 1.0, ((n0.x < 0.0) || (n0.x > 1.0)) || ((n0.y < 0.0) || (n0.y > 1.0)))"),
        "{text}"
    );
    // A field named after a keyword of another language keeps one name everywhere.
    assert!(text.contains("    sample_: f32,"), "{text}");
}

#[test]
fn gradient_samples_flip_their_v() {
    let (text, _) = wgsl(&fixtures::gradient_sample());
    assert!(
        text.contains("textureSampleGrad(shadewire_t0, shadewire_s0, vec2<f32>(n0.x, 1.0 - n0.y), vec2<f32>(n1.x, -n1.y), vec2<f32>(n2.x, -n2.y))"),
        "{text}"
    );
}

#[test]
fn library_functions_come_once_before_the_entry() {
    let (text, _) = wgsl(&fixtures::library_ops());
    assert_eq!(text.matches("fn shadewire_worley_noise_vec3_3d(").count(), 1, "{text}");
    let entry = text.find("fn shadewire_eval_surface").expect("entry");
    assert!(text.find("fn shadewire_worley_noise_vec3_3d").expect("worley") < entry, "{text}");
    // Library comments survive.
    assert!(text.contains("// mx_bjfinal (Rust bjfinal)"), "{text}");
}

#[test]
fn closure_functions_call_the_lobes() {
    let (text, _) = wgsl(&fixtures::every_lobe());
    for f in [
        "fn shadewire_eval_bsdf(",
        "fn shadewire_eval_albedo(",
        "fn shadewire_eval_emission(",
        "fn shadewire_eval_ambient(",
        "fn shadewire_mx_fresnel_airy(",
    ] {
        assert!(text.contains(f), "{f} missing: {text}");
    }
    assert!(
        text.contains("let c4: ShadewireBsdf = shadewire_dielectric_bsdf_light(to_light, to_viewer, surface.c4_weight, surface.c4_tint, surface.c4_ior"),
        "{text}"
    );
    assert!(text.contains("surface.c4_tangent, 2i);"), "scatter_mode RT is 2: {text}");
    assert!(
        text.contains("let c7: ShadewireBsdf = shadewire_sheen_bsdf_albedo(to_viewer, surface.c7_weight, surface.c7_color, surface.c7_roughness, surface.c7_normal, 1i);"),
        "{text}"
    );
    assert!(
        text.contains("let c8: ShadewireBsdf = shadewire_bsdf_none();  // chiang_hair_bsdf: not evaluated"),
        "{text}"
    );
    // An input the material leaves out takes the nodedef default.
    assert!(
        text.contains("shadewire_conductor_bsdf_light(to_light, to_viewer, surface.c5_weight, surface.c5_ior, surface.c5_extinction, surface.c5_roughness, false, 0.0, 1.5, surface.c5_normal, vec3<f32>(0.0))"),
        "{text}"
    );
    assert!(
        text.contains("shadewire_generalized_schlick_edf(to_viewer, shading_normal, vec3<f32>(1.0), vec3<f32>(1.0), surface.edf_exponent, c"),
        "{text}"
    );
    let root = fixtures::every_lobe().surface.closure.unwrap().bsdf.unwrap();
    let albedo = text.split("fn shadewire_eval_albedo").nth(1).unwrap();
    assert!(albedo.contains(&format!("return c{root}.response;")), "{albedo}");
}

#[test]
fn an_empty_tree_scatters_and_emits_nothing() {
    let (text, _) = wgsl(&fixtures::empty_closure());
    assert!(text.contains("return shadewire_bsdf_none();"), "{text}");
    assert!(text.contains("return shadewire_bsdf_none().response;"), "{text}");
    assert!(text.contains("return vec3<f32>(0.0);"), "{text}");
}

#[test]
fn no_closure_tree_no_closure_code() {
    let mut dag = Dag::new("gltf_pbr");
    fixtures::surface_field(&mut dag, "base_color", Type::Color3);
    let (text, _) = wgsl(&dag);
    assert!(!text.contains("ShadewireBsdf"), "{text}");
}

/// The fragment entry `validates` appends makes naga check stage rules on everything `shadewire_eval_surface`
/// reaches: a compute-only builtin is refused.
#[test]
#[should_panic(expected = "Stage")]
fn validates_checks_the_fragment_stage() {
    let dag = Dag::new("m");
    let text =
        "struct M { x: f32 }\nfn shadewire_eval_surface() -> M {\n    workgroupBarrier();\n    return M(1.0);\n}\n";
    check(text, &reflection(&dag)).map_err(|e| panic!("{e}")).unwrap();
}

#[test]
fn naga_validates_accepts_a_plain_function() {
    naga("fn f() -> f32 { return 1.0; }").unwrap();
}
