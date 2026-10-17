//! Distilling closure trees to `open_pbr_surface`: one test per mapping row, on trees built from shader nodes the
//! way the MDL and Blender front ends build them, and on MaterialX documents.

use shadewire::interpret::{NoTextures, eval_surface};
use shadewire::{Dag, Error, Value};

use super::tests::{Zeros, inline};
use crate::document::{Document, PortType};
use crate::elaborate::{Arg, Elaborator, elaborate};
use crate::index::Index;
use crate::library::standard;
use crate::options::IrOptions;
use crate::surface::fields;
use crate::surface::finish::ShaderNode;
use crate::synthetic::Synthetic;

fn options() -> IrOptions {
    IrOptions { distill: true, values_as_uniforms: false, ..IrOptions::with_models(&["open_pbr_surface"]) }
}

fn f(x: f32) -> Arg {
    Arg::Value(Value::Float(x))
}

fn c(r: f32, g: f32, b: f32) -> Arg {
    Arg::Value(Value::Color3([r, g, b]))
}

fn v2(x: f32, y: f32) -> Arg {
    Arg::Value(Value::Vector2([x, y]))
}

fn text(t: &str) -> Arg {
    Arg::Text(t.to_string())
}

/// A lobe or operator node of closure type `closure` (`BSDF`, `EDF`, `VDF`).
fn node(name: &str, category: &str, closure: &str, inputs: Vec<(&str, Arg)>, links: Vec<(&str, &str)>) -> ShaderNode {
    ShaderNode {
        name: name.to_string(),
        category: category.to_string(),
        port_type: PortType::Closure(closure.to_string()),
        inputs: inputs.into_iter().map(|(n, a)| (n.to_string(), a)).collect(),
        links: links.into_iter().map(|(n, t)| (n.to_string(), t.to_string())).collect(),
    }
}

fn bsdf(name: &str, category: &str, inputs: Vec<(&str, Arg)>) -> ShaderNode {
    node(name, category, "BSDF", inputs, Vec::new())
}

fn layer(name: &str, top: &str, base: &str) -> ShaderNode {
    node(name, "layer", "BSDF", Vec::new(), vec![("top", top), ("base", base)])
}

fn mix(name: &str, fg: Option<&str>, bg: Option<&str>, weight: Arg) -> ShaderNode {
    let mut links = Vec::new();
    if let Some(fg) = fg {
        links.push(("fg", fg));
    }
    if let Some(bg) = bg {
        links.push(("bg", bg));
    }
    node(name, "mix", "BSDF", vec![("mix", weight)], links)
}

fn surface(bsdf: Option<&str>, edf: Option<&str>, inputs: Vec<(&str, Arg)>) -> ShaderNode {
    let mut links = Vec::new();
    if let Some(b) = bsdf {
        links.push(("bsdf".to_string(), b.to_string()));
    }
    if let Some(e) = edf {
        links.push(("edf".to_string(), e.to_string()));
    }
    ShaderNode {
        name: "surface".into(),
        category: "surface".into(),
        port_type: PortType::SurfaceShader,
        inputs: inputs.into_iter().map(|(n, a)| (n.to_string(), a)).collect(),
        links,
    }
}

fn distilled_with(mut nodes: Vec<ShaderNode>, root: &str, o: &IrOptions) -> Dag {
    let s = Synthetic::new();
    let e = Elaborator::standalone(&s, o);
    nodes.push(surface(Some(root), None, Vec::new()));
    e.finish_closure(nodes, "surface").unwrap()
}

fn distilled(nodes: Vec<ShaderNode>, root: &str) -> Dag {
    distilled_with(nodes, root, &options())
}

fn value(dag: &Dag, name: &str) -> Value {
    let values = eval_surface(dag, &mut Zeros, &NoTextures);
    values.into_iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no field {name}")).1
}

#[track_caller]
fn near(dag: &Dag, name: &str, expected: &[f32]) {
    let got = value(dag, name).floats();
    assert_eq!(got.len(), expected.len(), "{name}: {got:?}");
    for (g, e) in got.iter().zip(expected) {
        assert!((g - e).abs() < 1e-4, "{name}: {got:?}, expected {expected:?}");
    }
}

fn open_pbr_field_count() -> usize {
    let doc = Document::empty();
    let index = Index::new(&doc, standard());
    fields(index.nodedef("ND_open_pbr_surface_surfaceshader").unwrap()).len()
}

/// open_pbr_anisotropy: the alpha pair of `roughness` and `anisotropy`.
fn alpha(roughness: f32, anisotropy: f32) -> Arg {
    let inv = 1.0 - anisotropy;
    let x = roughness * roughness * (2.0 / (1.0 + inv * inv)).sqrt();
    v2(x, x * inv)
}

#[test]
fn a_single_diffuse_is_exact() {
    let dag = distilled(
        vec![bsdf(
            "d",
            "oren_nayar_diffuse_bsdf",
            vec![("weight", f(0.7)), ("color", c(0.9, 0.2, 0.1)), ("roughness", f(0.4))],
        )],
        "d",
    );
    assert_eq!(dag.surface.model, "open_pbr_surface");
    assert!(dag.surface.closure.is_none());
    assert_eq!(dag.surface.fields.len(), open_pbr_field_count());
    assert!(dag.approximations.is_empty(), "{:?}", dag.approximations);
    near(&dag, "base_weight", &[0.7]);
    near(&dag, "base_color", &[0.9, 0.2, 0.1]);
    near(&dag, "base_diffuse_roughness", &[0.4]);
    for zero in ["base_metalness", "specular_weight", "transmission_weight", "subsurface_weight", "fuzz_weight"] {
        near(&dag, zero, &[0.0]);
    }
    for zero in ["coat_weight", "thin_film_weight", "emission_luminance"] {
        near(&dag, zero, &[0.0]);
    }
    near(&dag, "geometry_opacity", &[1.0]);
}

#[test]
fn burley_diffuse_is_the_diffuse_family() {
    let dag = distilled(
        vec![bsdf("d", "burley_diffuse_bsdf", vec![("color", c(0.3, 0.4, 0.5)), ("roughness", f(0.2))])],
        "d",
    );
    near(&dag, "base_color", &[0.3, 0.4, 0.5]);
    near(&dag, "base_diffuse_roughness", &[0.2]);
    near(&dag, "base_weight", &[1.0]);
}

#[test]
fn a_dielectric_over_a_diffuse_is_exact() {
    let dag = distilled(
        vec![
            bsdf(
                "spec",
                "dielectric_bsdf",
                vec![
                    ("weight", f(0.8)),
                    ("tint", c(0.9, 0.9, 1.0)),
                    ("ior", f(1.45)),
                    ("roughness", v2(0.09, 0.09)),
                    ("scatter_mode", text("R")),
                ],
            ),
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.5, 0.25, 0.125))]),
            layer("l", "spec", "d"),
        ],
        "l",
    );
    assert!(dag.approximations.is_empty(), "{:?}", dag.approximations);
    near(&dag, "specular_weight", &[0.8]);
    near(&dag, "specular_color", &[0.9, 0.9, 1.0]);
    near(&dag, "specular_ior", &[1.45]);
    near(&dag, "specular_roughness", &[0.3]);
    near(&dag, "specular_roughness_anisotropy", &[0.0]);
    // open_pbr_surface layers its specular over the base itself: the base is not darkened here.
    near(&dag, "base_weight", &[1.0]);
    near(&dag, "base_color", &[0.5, 0.25, 0.125]);
    near(&dag, "base_metalness", &[0.0]);
    near(&dag, "coat_weight", &[0.0]);
}

#[test]
fn an_anisotropic_roughness_inverts_open_pbr_anisotropy() {
    let dag = distilled(
        vec![
            bsdf("spec", "dielectric_bsdf", vec![("roughness", alpha(0.5, 0.6)), ("scatter_mode", text("R"))]),
            bsdf("d", "oren_nayar_diffuse_bsdf", Vec::new()),
            layer("l", "spec", "d"),
        ],
        "l",
    );
    near(&dag, "specular_roughness", &[0.5]);
    near(&dag, "specular_roughness_anisotropy", &[0.6]);
}

#[test]
fn a_dielectric_reflecting_and_transmitting_is_glass() {
    let dag = distilled(
        vec![bsdf(
            "glass",
            "dielectric_bsdf",
            vec![
                ("tint", c(0.8, 0.9, 1.0)),
                ("ior", f(1.52)),
                ("roughness", v2(0.0, 0.0)),
                ("scatter_mode", text("RT")),
            ],
        )],
        "glass",
    );
    near(&dag, "transmission_weight", &[1.0]);
    near(&dag, "transmission_color", &[0.8, 0.9, 1.0]);
    near(&dag, "specular_weight", &[1.0]);
    near(&dag, "specular_ior", &[1.52]);
    near(&dag, "specular_roughness", &[0.0]);
    near(&dag, "base_metalness", &[0.0]);
}

#[test]
fn a_transmission_mixed_with_a_diffuse_weighs_by_the_mix() {
    let dag = distilled(
        vec![
            bsdf(
                "t",
                "dielectric_bsdf",
                vec![("tint", c(0.2, 0.4, 0.6)), ("ior", f(1.33)), ("scatter_mode", text("T"))],
            ),
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.5, 0.5, 0.5))]),
            mix("m", Some("t"), Some("d"), f(0.25)),
        ],
        "m",
    );
    near(&dag, "transmission_weight", &[0.25]);
    near(&dag, "transmission_color", &[0.2, 0.4, 0.6]);
    near(&dag, "specular_ior", &[1.33]);
    near(&dag, "base_color", &[0.5, 0.5, 0.5]);
}

#[test]
fn a_conductor_is_metal_with_its_normal_incidence_reflectivity() {
    let (n, k) = ([0.2f32, 0.9, 1.1], [3.9f32, 2.4, 2.2]);
    let dag = distilled(
        vec![bsdf(
            "m",
            "conductor_bsdf",
            vec![("ior", c(n[0], n[1], n[2])), ("extinction", c(k[0], k[1], k[2])), ("roughness", v2(0.04, 0.04))],
        )],
        "m",
    );
    let f0: Vec<f32> =
        (0..3).map(|i| ((n[i] - 1.0).powi(2) + k[i] * k[i]) / ((n[i] + 1.0).powi(2) + k[i] * k[i])).collect();
    near(&dag, "base_metalness", &[1.0]);
    near(&dag, "base_color", &f0);
    near(&dag, "base_weight", &[1.0]);
    near(&dag, "specular_roughness", &[0.2]);
    // The metal keeps its full reflection: open_pbr_surface scales it by specular_weight.
    near(&dag, "specular_weight", &[1.0]);
}

#[test]
fn a_schlick_lobe_that_is_no_layer_top_is_metal() {
    let dag = distilled(
        vec![bsdf(
            "m",
            "generalized_schlick_bsdf",
            vec![("color0", c(0.9, 0.6, 0.2)), ("color82", c(0.95, 0.9, 0.8)), ("roughness", v2(0.25, 0.25))],
        )],
        "m",
    );
    near(&dag, "base_metalness", &[1.0]);
    near(&dag, "base_color", &[0.9, 0.6, 0.2]);
    near(&dag, "specular_color", &[0.95, 0.9, 0.8]);
    near(&dag, "specular_roughness", &[0.5]);
}

#[test]
fn a_metal_mixed_with_a_diffuse_sets_metalness_and_blends_the_base_color() {
    let dag = distilled(
        vec![
            bsdf("m", "generalized_schlick_bsdf", vec![("color0", c(1.0, 0.0, 0.0))]),
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.0, 0.0, 1.0))]),
            mix("x", Some("m"), Some("d"), f(0.3)),
        ],
        "x",
    );
    near(&dag, "base_metalness", &[0.3]);
    near(&dag, "base_color", &[0.3, 0.0, 0.7]);
    near(&dag, "base_weight", &[1.0]);
}

#[test]
fn a_fresnel_ramp_layered_over_a_diffuse_is_the_specular_layer() {
    // MDL's custom_curve_layer over a glossy lobe: normal reflectivity 0.04 at weight 1 is an ior of 1.5.
    let dag = distilled(
        vec![
            bsdf(
                "s",
                "generalized_schlick_bsdf",
                vec![("color0", c(0.04, 0.04, 0.04)), ("color90", c(1.0, 1.0, 1.0)), ("roughness", v2(0.16, 0.16))],
            ),
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.2, 0.2, 0.2))]),
            layer("l", "s", "d"),
        ],
        "l",
    );
    near(&dag, "base_metalness", &[0.0]);
    near(&dag, "specular_weight", &[1.0]);
    near(&dag, "specular_ior", &[1.5]);
    near(&dag, "specular_color", &[1.0, 1.0, 1.0]);
    near(&dag, "specular_roughness", &[0.4]);
    near(&dag, "base_color", &[0.2, 0.2, 0.2]);
}

#[test]
fn a_metal_layered_over_a_dielectric_is_metalness_as_mdl_weighted_layer_builds_it() {
    // OmniPBR: weighted_layer(metalness, tint(base, glossy), custom_curve_layer(0.08, specular_level, glossy, diffuse)).
    let base = c(0.6, 0.3, 0.1);
    let dag = distilled(
        vec![
            bsdf(
                "metal",
                "generalized_schlick_bsdf",
                vec![("color0", c(1.0, 1.0, 1.0)), ("color82", c(1.0, 1.0, 1.0)), ("color90", c(1.0, 1.0, 1.0))],
            ),
            node("tinted", "multiply", "BSDF", vec![("in2", base.clone())], vec![("in1", "metal")]),
            bsdf(
                "s",
                "generalized_schlick_bsdf",
                vec![("weight", f(0.5)), ("color0", c(0.08, 0.08, 0.08)), ("color90", c(1.0, 1.0, 1.0))],
            ),
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", base)]),
            layer("cc", "s", "d"),
            layer("over", "tinted", "cc"),
            mix("omni", Some("over"), Some("cc"), f(0.4)),
        ],
        "omni",
    );
    near(&dag, "base_metalness", &[0.4]);
    near(&dag, "base_color", &[0.6, 0.3, 0.1]);
    near(&dag, "base_weight", &[1.0]);
    // A metal present keeps specular_weight at one; the dielectric's weight scales its F0 instead: 0.5 × 0.08.
    near(&dag, "specular_weight", &[1.0]);
    near(&dag, "specular_ior", &[1.5]);
    near(&dag, "coat_weight", &[0.0]);
}

#[test]
fn a_transmission_layered_over_a_specular_base_is_no_coat_and_hides_the_base() {
    // OmniSurface: weighted_layer(transmission_weight, glass, specular over diffuse).
    let dag = distilled(
        vec![
            bsdf(
                "glass",
                "dielectric_bsdf",
                vec![("ior", f(1.5)), ("tint", c(0.9, 0.95, 1.0)), ("scatter_mode", text("RT"))],
            ),
            bsdf("s", "generalized_schlick_bsdf", vec![("color0", c(0.04, 0.04, 0.04)), ("color90", c(1.0, 1.0, 1.0))]),
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.8, 0.8, 0.8))]),
            layer("base", "s", "d"),
            layer("over", "glass", "base"),
            mix("w", Some("over"), Some("base"), f(0.5)),
        ],
        "w",
    );
    near(&dag, "coat_weight", &[0.0]);
    near(&dag, "transmission_weight", &[0.5]);
    near(&dag, "transmission_color", &[0.9, 0.95, 1.0]);
    near(&dag, "specular_ior", &[1.5]);
    near(&dag, "base_weight", &[1.0]);
    near(&dag, "base_metalness", &[0.0]);
}

#[test]
fn a_sheen_layer_is_fuzz_over_an_undarkened_base() {
    let dag = distilled(
        vec![
            bsdf("s", "sheen_bsdf", vec![("weight", f(0.6)), ("color", c(0.4, 0.3, 0.2)), ("roughness", f(0.7))]),
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.1, 0.1, 0.1))]),
            layer("l", "s", "d"),
        ],
        "l",
    );
    near(&dag, "fuzz_weight", &[0.6]);
    near(&dag, "fuzz_color", &[0.4, 0.3, 0.2]);
    near(&dag, "fuzz_roughness", &[0.7]);
    near(&dag, "base_weight", &[1.0]);
    near(&dag, "base_color", &[0.1, 0.1, 0.1]);
}

#[test]
fn a_subsurface_mixed_with_a_diffuse_is_subsurface_weight() {
    let dag = distilled(
        vec![
            bsdf(
                "ss",
                "subsurface_bsdf",
                vec![("color", c(0.7, 0.2, 0.2)), ("radius", c(2.0, 1.0, 0.5)), ("anisotropy", f(0.3))],
            ),
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.7, 0.2, 0.2))]),
            mix("m", Some("ss"), Some("d"), f(0.6)),
        ],
        "m",
    );
    near(&dag, "subsurface_weight", &[0.6]);
    near(&dag, "subsurface_color", &[0.7, 0.2, 0.2]);
    near(&dag, "subsurface_radius", &[2.0]);
    near(&dag, "subsurface_radius_scale", &[1.0, 0.5, 0.25]);
    near(&dag, "subsurface_scatter_anisotropy", &[0.3]);
}

#[test]
fn a_translucent_lobe_is_subsurface() {
    let dag = distilled(vec![bsdf("t", "translucent_bsdf", vec![("color", c(0.3, 0.6, 0.9))])], "t");
    near(&dag, "subsurface_weight", &[1.0]);
    near(&dag, "subsurface_color", &[0.3, 0.6, 0.9]);
    assert!(dag.approximations.is_empty(), "{:?}", dag.approximations);
}

#[test]
fn a_dielectric_over_a_specular_layer_is_the_coat() {
    let dag = distilled(
        vec![
            bsdf(
                "coat",
                "dielectric_bsdf",
                vec![("weight", f(0.9)), ("ior", f(1.6)), ("roughness", v2(0.01, 0.01)), ("scatter_mode", text("R"))],
            ),
            bsdf(
                "spec",
                "dielectric_bsdf",
                vec![("ior", f(1.4)), ("roughness", v2(0.25, 0.25)), ("scatter_mode", text("R"))],
            ),
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.1, 0.6, 0.9))]),
            layer("base", "spec", "d"),
            layer("coated", "coat", "base"),
        ],
        "coated",
    );
    near(&dag, "coat_weight", &[0.9]);
    near(&dag, "coat_ior", &[1.6]);
    near(&dag, "coat_roughness", &[0.1]);
    near(&dag, "coat_color", &[1.0, 1.0, 1.0]);
    near(&dag, "specular_ior", &[1.4]);
    near(&dag, "specular_roughness", &[0.5]);
    near(&dag, "specular_weight", &[1.0]);
    near(&dag, "base_color", &[0.1, 0.6, 0.9]);
}

#[test]
fn a_dielectric_over_a_bare_metal_is_a_coat() {
    let dag = distilled(
        vec![
            bsdf("coat", "dielectric_bsdf", vec![("weight", f(1.0)), ("ior", f(1.5)), ("scatter_mode", text("R"))]),
            bsdf("m", "conductor_bsdf", Vec::new()),
            layer("l", "coat", "m"),
        ],
        "l",
    );
    near(&dag, "coat_weight", &[1.0]);
    near(&dag, "coat_ior", &[1.5]);
    near(&dag, "base_metalness", &[1.0]);
}

#[test]
fn a_thin_film_on_the_specular_is_thin_film_fields() {
    let dag = distilled(
        vec![
            bsdf(
                "spec",
                "dielectric_bsdf",
                vec![("thinfilm_thickness", f(400.0)), ("thinfilm_ior", f(1.3)), ("scatter_mode", text("R"))],
            ),
            bsdf("d", "oren_nayar_diffuse_bsdf", Vec::new()),
            layer("l", "spec", "d"),
        ],
        "l",
    );
    near(&dag, "thin_film_weight", &[1.0]);
    near(&dag, "thin_film_thickness", &[0.4]);
    near(&dag, "thin_film_ior", &[1.3]);
}

fn emitting(edf: Vec<ShaderNode>, root: &str) -> Dag {
    let s = Synthetic::new();
    let o = options();
    let e = Elaborator::standalone(&s, &o);
    let mut nodes = edf;
    nodes.push(bsdf("d", "oren_nayar_diffuse_bsdf", Vec::new()));
    nodes.push(surface(Some("d"), Some(root), Vec::new()));
    e.finish_closure(nodes, "surface").unwrap()
}

#[test]
fn a_uniform_edf_is_emission_color_and_luminance() {
    let dag = emitting(vec![node("e", "uniform_edf", "EDF", vec![("color", c(2.0, 1.0, 0.5))], Vec::new())], "e");
    near(&dag, "emission_luminance", &[2.0]);
    near(&dag, "emission_color", &[1.0, 0.5, 0.25]);
    let scaled = emitting(
        vec![
            node("e", "uniform_edf", "EDF", vec![("color", c(2.0, 1.0, 0.5))], Vec::new()),
            node("x", "multiply", "EDF", vec![("in2", f(3.0))], vec![("in1", "e")]),
        ],
        "x",
    );
    near(&scaled, "emission_luminance", &[6.0]);
    near(&scaled, "emission_color", &[1.0, 0.5, 0.25]);
}

#[test]
fn a_conical_edf_is_uniform_emission_with_a_warning() {
    let dag = emitting(vec![node("e", "conical_edf", "EDF", vec![("color", c(1.0, 1.0, 1.0))], Vec::new())], "e");
    near(&dag, "emission_luminance", &[1.0]);
    assert!(dag.approximations.iter().any(|a| a.contains("conical_edf")), "{:?}", dag.approximations);
}

#[test]
fn the_surface_geometry_is_kept() {
    let s = Synthetic::new();
    let o = options();
    let e = Elaborator::standalone(&s, &o);
    let n = Arg::Value(Value::Vector3([0.0, 0.6, 0.8]));
    let t = Arg::Value(Value::Vector3([1.0, 0.0, 0.0]));
    let nodes = vec![
        bsdf("spec", "dielectric_bsdf", vec![("normal", n.clone()), ("tangent", t), ("scatter_mode", text("R"))]),
        bsdf("d", "oren_nayar_diffuse_bsdf", vec![("normal", n)]),
        layer("l", "spec", "d"),
        surface(Some("l"), None, vec![("opacity", f(0.5)), ("thin_walled", Arg::Value(Value::Boolean(true)))]),
    ];
    let dag = e.finish_closure(nodes, "surface").unwrap();
    near(&dag, "geometry_opacity", &[0.5]);
    assert_eq!(value(&dag, "geometry_thin_walled"), Value::Boolean(true));
    near(&dag, "geometry_normal", &[0.0, 0.6, 0.8]);
    near(&dag, "geometry_tangent", &[1.0, 0.0, 0.0]);
    assert!(dag.approximations.is_empty(), "{:?}", dag.approximations);
}

#[test]
fn lobes_with_different_normals_take_the_first_with_a_warning() {
    let dag = distilled(
        vec![
            bsdf("a", "oren_nayar_diffuse_bsdf", vec![("normal", Arg::Value(Value::Vector3([0.0, 0.0, 1.0])))]),
            bsdf("b", "oren_nayar_diffuse_bsdf", vec![("normal", Arg::Value(Value::Vector3([0.0, 1.0, 0.0])))]),
            mix("m", Some("a"), Some("b"), f(0.5)),
        ],
        "m",
    );
    near(&dag, "geometry_normal", &[0.0, 0.0, 1.0]);
    assert!(dag.approximations.iter().any(|a| a.contains("normal")), "{:?}", dag.approximations);
}

#[test]
fn a_mix_weighs_its_sides() {
    let dag = distilled(
        vec![
            bsdf("a", "oren_nayar_diffuse_bsdf", vec![("color", c(1.0, 0.0, 0.0)), ("roughness", f(1.0))]),
            bsdf("b", "oren_nayar_diffuse_bsdf", vec![("color", c(0.0, 0.0, 1.0))]),
            mix("m", Some("a"), Some("b"), f(0.25)),
        ],
        "m",
    );
    near(&dag, "base_color", &[0.25, 0.0, 0.75]);
    near(&dag, "base_diffuse_roughness", &[0.25]);
    near(&dag, "base_weight", &[1.0]);
}

#[test]
fn an_add_sums_and_normalises_per_family() {
    let dag = distilled(
        vec![
            bsdf("a", "oren_nayar_diffuse_bsdf", vec![("weight", f(0.5)), ("color", c(1.0, 0.0, 0.0))]),
            bsdf("b", "oren_nayar_diffuse_bsdf", vec![("weight", f(0.5)), ("color", c(0.0, 0.0, 1.0))]),
            node("sum", "add", "BSDF", Vec::new(), vec![("in1", "a"), ("in2", "b")]),
        ],
        "sum",
    );
    // Two half diffuses sum to one whole diffuse of their average color.
    near(&dag, "base_color", &[0.5, 0.0, 0.5]);
    near(&dag, "base_weight", &[1.0]);
}

#[test]
fn a_multiply_scales_the_weight_or_tints_the_color() {
    let scaled = distilled(
        vec![
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.8, 0.8, 0.8))]),
            node("x", "multiply", "BSDF", vec![("in2", f(0.5))], vec![("in1", "d")]),
        ],
        "x",
    );
    near(&scaled, "base_weight", &[0.5]);
    near(&scaled, "base_color", &[0.8, 0.8, 0.8]);
    let tinted = distilled(
        vec![
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.8, 0.8, 0.8))]),
            node("x", "multiply", "BSDF", vec![("in2", c(1.0, 0.5, 0.25))], vec![("in1", "d")]),
        ],
        "x",
    );
    near(&tinted, "base_weight", &[1.0]);
    near(&tinted, "base_color", &[0.8, 0.4, 0.2]);
}

#[test]
fn an_opaque_layer_top_hides_its_weight_of_the_base() {
    let dag = distilled(
        vec![
            bsdf("top", "oren_nayar_diffuse_bsdf", vec![("weight", f(0.4)), ("color", c(1.0, 0.0, 0.0))]),
            bsdf("m", "generalized_schlick_bsdf", vec![("color0", c(0.0, 0.0, 1.0))]),
            layer("l", "top", "m"),
        ],
        "l",
    );
    // The metal under a diffuse of weight 0.4 gets 1 - 0.4 of the surface.
    near(&dag, "base_metalness", &[0.6]);
    near(&dag, "base_color", &[0.4, 0.0, 0.6]);
}

#[test]
fn an_empty_side_of_a_mix_leaves_its_share_black() {
    let dag = distilled(
        vec![
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.5, 0.5, 0.5))]),
            mix("m", Some("d"), None, f(0.5)),
        ],
        "m",
    );
    near(&dag, "base_weight", &[0.5]);
    near(&dag, "base_color", &[0.5, 0.5, 0.5]);
}

#[test]
fn lobes_without_a_field_are_dropped_with_a_warning_naming_them() {
    let dag = distilled(
        vec![
            bsdf("h", "chiang_hair_bsdf", Vec::new()),
            bsdf("d", "oren_nayar_diffuse_bsdf", Vec::new()),
            node("sum", "add", "BSDF", Vec::new(), vec![("in1", "h"), ("in2", "d")]),
        ],
        "sum",
    );
    assert!(dag.approximations.iter().any(|a| a.contains("chiang_hair_bsdf")), "{:?}", dag.approximations);
    let vdf = distilled(
        vec![
            bsdf("t", "dielectric_bsdf", vec![("scatter_mode", text("T"))]),
            node("v", "absorption_vdf", "VDF", vec![("absorption", Arg::Value(Value::Vector3([0.1; 3])))], Vec::new()),
            node("l", "layer", "BSDF", Vec::new(), vec![("top", "t"), ("base", "v")]),
        ],
        "l",
    );
    assert!(vdf.approximations.iter().any(|a| a.contains("absorption_vdf")), "{:?}", vdf.approximations);
    near(&vdf, "transmission_weight", &[1.0]);
}

#[test]
fn a_family_whose_weight_is_zero_reads_its_defaults() {
    // The weight a uniform, so nothing folds: the division is guarded at run time.
    let s = Synthetic::new();
    let o = IrOptions { values_as_uniforms: true, ..options() };
    let mut e = Elaborator::standalone(&s, &o);
    let zero = e.authored("w", Value::Float(0.0), "w");
    let nodes = vec![
        bsdf("ss1", "subsurface_bsdf", vec![("color", c(0.1, 0.2, 0.3))]),
        bsdf("ss2", "subsurface_bsdf", vec![("color", c(0.3, 0.2, 0.1))]),
        mix("ss", Some("ss1"), Some("ss2"), f(0.5)),
        bsdf("m", "conductor_bsdf", Vec::new()),
        bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.5, 0.5, 0.5))]),
        mix("x", Some("ss"), Some("d"), Arg::Node(zero)),
        mix("y", Some("m"), Some("x"), Arg::Node(zero)),
        surface(Some("y"), None, Vec::new()),
    ];
    let dag = e.finish_closure(nodes, "surface").unwrap();
    for (name, v) in eval_surface(&dag, &mut Zeros, &NoTextures) {
        assert!(v.floats().iter().all(|x| x.is_finite()), "{name}: {v:?}");
    }
    near(&dag, "subsurface_weight", &[0.0]);
    near(&dag, "subsurface_color", &[0.8, 0.8, 0.8]);
    near(&dag, "base_metalness", &[0.0]);
    near(&dag, "base_color", &[0.5, 0.5, 0.5]);
}

#[test]
fn without_distill_a_closure_tree_is_unchanged() {
    let o = IrOptions { values_as_uniforms: false, ..IrOptions::with_models(&["surface"]) };
    let dag = distilled_with(vec![bsdf("d", "oren_nayar_diffuse_bsdf", Vec::new())], "d", &o);
    assert_eq!(dag.surface.model, "surface");
    assert!(dag.surface.closure.is_some());
    let s = Synthetic::new();
    let o = IrOptions { distill: false, ..options() };
    let e = Elaborator::standalone(&s, &o);
    let nodes = vec![bsdf("d", "oren_nayar_diffuse_bsdf", Vec::new()), surface(Some("d"), None, Vec::new())];
    assert!(matches!(e.finish_closure(nodes, "surface"), Err(Error::NoTranslation { .. })));
}

#[test]
fn distill_needs_open_pbr_surface_accepted() {
    let s = Synthetic::new();
    let o = IrOptions { distill: true, ..IrOptions::with_models(&["standard_surface"]) };
    let e = Elaborator::standalone(&s, &o);
    let nodes = vec![bsdf("d", "oren_nayar_diffuse_bsdf", Vec::new()), surface(Some("d"), None, Vec::new())];
    assert!(matches!(e.finish_closure(nodes, "surface"), Err(Error::NoTranslation { .. })));
}

#[test]
fn distill_wins_over_an_accepted_surface_model() {
    let o = IrOptions {
        distill: true,
        values_as_uniforms: false,
        ..IrOptions::with_models(&["surface", "open_pbr_surface"])
    };
    let dag = distilled_with(vec![bsdf("d", "oren_nayar_diffuse_bsdf", Vec::new())], "d", &o);
    assert_eq!(dag.surface.model, "open_pbr_surface");
}

#[test]
fn a_document_surface_node_and_an_unaccepted_model_distill() {
    let doc = inline(
        r#"
        <oren_nayar_diffuse_bsdf name="d" type="BSDF">
          <input name="color" type="color3" value="0.2, 0.4, 0.6"/>
        </oren_nayar_diffuse_bsdf>
        <surface name="s" type="surfaceshader">
          <input name="bsdf" type="BSDF" nodename="d"/>
        </surface>
        <surfacematerial name="m" type="material">
          <input name="surfaceshader" type="surfaceshader" nodename="s"/>
        </surfacematerial>"#,
    );
    let dag = elaborate(&doc, standard(), &options(), None).unwrap();
    assert_eq!(dag.surface.model, "open_pbr_surface");
    near(&dag, "base_color", &[0.2, 0.4, 0.6]);
    let doc = inline(
        r#"
        <standard_surface name="ss" type="surfaceshader">
          <input name="base_color" type="color3" value="0.9, 0.5, 0.1"/>
          <input name="metalness" type="float" value="1"/>
        </standard_surface>
        <surfacematerial name="m" type="material">
          <input name="surfaceshader" type="surfaceshader" nodename="ss"/>
        </surfacematerial>"#,
    );
    let dag = elaborate(&doc, standard(), &options(), None).unwrap();
    assert_eq!(dag.surface.model, "open_pbr_surface");
    near(&dag, "base_metalness", &[1.0]);
    // standard_surface's metal is an artistic-ior conductor: its normal-incidence reflectivity is the base color.
    near(&dag, "base_color", &[0.9, 0.5, 0.1]);
}

#[test]
fn a_half_and_half_mix_of_diffuse_and_conductor_is_half_metal() {
    let (n, k) = (0.2f32, 3.0f32);
    let dag = distilled(
        vec![
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.0, 0.0, 1.0))]),
            bsdf("m", "conductor_bsdf", vec![("ior", c(n, n, n)), ("extinction", c(k, k, k))]),
            mix("x", Some("m"), Some("d"), f(0.5)),
        ],
        "x",
    );
    let f0 = ((n - 1.0).powi(2) + k * k) / ((n + 1.0).powi(2) + k * k);
    near(&dag, "base_metalness", &[0.5]);
    near(&dag, "base_color", &[0.5 * f0, 0.5 * f0, 0.5 + 0.5 * f0]);
    near(&dag, "base_weight", &[1.0]);
}

/// A metal at half the surface next to a dielectric specular over a diffuse.
fn half_metal(color82: Option<Arg>) -> Dag {
    let gold = c(0.9, 0.6, 0.2);
    let mut metal = vec![("color0", gold.clone()), ("color90", gold.clone())];
    metal.push(("color82", color82.unwrap_or(gold)));
    distilled(
        vec![
            bsdf("m", "generalized_schlick_bsdf", metal),
            bsdf("spec", "dielectric_bsdf", vec![("ior", f(1.5)), ("scatter_mode", text("R"))]),
            bsdf("d", "oren_nayar_diffuse_bsdf", vec![("color", c(0.5, 0.5, 0.5))]),
            layer("l", "spec", "d"),
            mix("x", Some("m"), Some("l"), f(0.5)),
        ],
        "x",
    )
}

#[test]
fn a_metal_whose_edge_is_its_face_leaves_the_dielectric_specular_white() {
    let dag = half_metal(None);
    near(&dag, "base_metalness", &[0.5]);
    near(&dag, "base_color", &[0.7, 0.55, 0.35]);
    near(&dag, "specular_color", &[1.0, 1.0, 1.0]);
}

#[test]
fn a_metal_edge_tint_weighs_by_the_metal_share_of_the_specular() {
    let dag = half_metal(Some(c(0.5, 0.5, 0.5)));
    // The metal's flow 0.5 and the dielectric's coverage 0.5: half the edge tint.
    near(&dag, "specular_color", &[0.75, 0.75, 0.75]);
}

#[test]
fn a_lobe_carrying_dispersion_is_noted() {
    use std::collections::BTreeMap;

    use shadewire::{ClosureGraph, ClosureNode, ClosureType, Node, NodeKind, SurfaceField};

    let mut dag = Dag::new("surface");
    let field = |dag: &mut Dag, name: &str, v: Value| {
        let ty = v.ty();
        let node = dag.add(Node { kind: NodeKind::Const(v), ty, origin: name.to_string() });
        dag.surface.fields.push(SurfaceField { name: name.to_string(), ty, node });
    };
    field(&mut dag, "c0_ior", Value::Float(1.5));
    field(&mut dag, "c0_dispersion", Value::Float(0.5));
    let mut graph = ClosureGraph::default();
    let inputs = BTreeMap::from([
        ("ior".to_string(), "c0_ior".to_string()),
        ("dispersion".to_string(), "c0_dispersion".to_string()),
    ]);
    let settings = BTreeMap::from([("scatter_mode".to_string(), "RT".to_string())]);
    let glass = graph.add(ClosureNode::Lobe {
        kind: "dielectric_bsdf".into(),
        ty: ClosureType::Bsdf,
        inputs,
        settings,
        closures: BTreeMap::new(),
    });
    graph.bsdf = Some(glass);
    dag.surface.closure = Some(graph);
    crate::surface::distill::distill(&mut dag).unwrap();
    assert_eq!(dag.surface.model, "open_pbr_surface");
    assert!(dag.approximations.iter().any(|a| a.contains("dispersion")), "{:?}", dag.approximations);
    near(&dag, "transmission_weight", &[1.0]);
}
