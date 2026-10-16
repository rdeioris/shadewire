use shadewire::optimize::optimize;
use shadewire::{LOBE_MODELS, Value};

use crate::{IrOptions, MaterialIr, Resolver, to_ir};

pub(crate) fn lower(text: &str) -> MaterialIr {
    let options = IrOptions::with_models(&LOBE_MODELS);
    to_ir(text, None, None, &options, &Resolver::default()).unwrap_or_else(|e| panic!("{e}"))
}

pub(crate) fn lower_constants(text: &str) -> MaterialIr {
    let mut options = IrOptions::with_models(&LOBE_MODELS);
    options.values_as_uniforms = false;
    to_ir(text, None, None, &options, &Resolver::default()).unwrap_or_else(|e| panic!("{e}"))
}

/// The value of surface field `name` when it folds to a constant.
pub(crate) fn field_value(ir: &MaterialIr, name: &str) -> Value {
    let dag = optimize(&ir.dag);
    let field =
        dag.surface.fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no field {name}: {}", dag.dump()));
    match &dag.node(field.node).kind {
        shadewire::NodeKind::Const(v) => v.clone(),
        other => panic!("{name} is {other:?}\n{}", dag.dump()),
    }
}

/// A material whose diffuse tint is `expr` (a color), with `prelude` before it.
fn tint_of(prelude: &str, expr: &str) -> Value {
    let text = format!(
        "mdl 1.7;\nimport ::df::*;\nimport ::math::*;\nimport ::state::*;\n{prelude}\nexport material m() = material(surface: material_surface(scattering: df::diffuse_reflection_bsdf(tint: {expr})));\n"
    );
    let ir = lower_constants(&text);
    field_value(&ir, "c0_color")
}

#[test]
fn a_diffuse_material_is_one_lobe() {
    let ir = lower(
        "mdl 1.7;\nimport ::df::*;\nexport material m(color tint = color(0.5, 0.25, 1.0)) = material(surface: material_surface(scattering: df::diffuse_reflection_bsdf(tint: tint)));\n",
    );
    let closure = ir.dag.surface.closure.as_ref().unwrap();
    assert_eq!(
        closure.dump().lines().next().unwrap(),
        "c0 = oren_nayar_diffuse_bsdf(color=c0_color, energy_compensation=c0_energy_compensation, normal=c0_normal, roughness=c0_roughness, weight=c0_weight)"
    );
    let uniform =
        ir.dag.nodes.iter().find(|n| matches!(&n.kind, shadewire::NodeKind::Uniform { name, .. } if name == "tint"));
    assert!(uniform.is_some(), "{}", ir.dag.dump());
}

#[test]
fn arithmetic_folds() {
    assert_eq!(tint_of("", "color(0.25) * 2.0 + color(0.0, 0.5, 1.0)"), Value::Color3([0.5, 1.0, 1.5]));
    assert_eq!(tint_of("", "color(float3(1, 2, 3).y / 4)"), Value::Color3([0.5; 3]));
    assert_eq!(tint_of("", "color(math::lerp(0.0, 2.0, 0.25))"), Value::Color3([0.5; 3]));
    assert_eq!(tint_of("", "color(float(7 / 2) + float(-7 % 3) * 10.0)"), Value::Color3([-7.0; 3]));
}

#[test]
fn functions_with_control_flow() {
    let prelude = r#"
float f(float x) {
    float y = 0.0;
    for (int i = 0; i < 4; ++i) {
        if (i == 2)
            continue;
        y += x;
    }
    return y;
}
int g(int k) {
    switch (k) {
        case 0: return 10;
        case 1:
        case 2: return 20;
        default: break;
    }
    int n = 0;
    while (n < 5) n++;
    return n;
}
struct pair { float a; float b = 3.0; };
float h(pair p) { return p.a * p.b; }
"#;
    assert_eq!(tint_of(prelude, "color(f(0.25))"), Value::Color3([0.75; 3]));
    assert_eq!(tint_of(prelude, "color(float(g(0) + g(2) + g(7)))"), Value::Color3([35.0; 3]));
    assert_eq!(tint_of(prelude, "color(h(pair(2.0)))"), Value::Color3([6.0; 3]));
}

#[test]
fn branches_on_run_time_values_are_predicated() {
    let ir = lower(
        r#"mdl 1.7;
import ::df::*;
import ::state::*;
float pick(float x) {
    if (x > 0.5)
        return 1.0;
    return 0.25;
}
export material m() = material(surface: material_surface(scattering: df::diffuse_reflection_bsdf(tint: color(pick(state::texture_coordinate(0).x)))));
"#,
    );
    let dag = optimize(&ir.dag);
    let dump = dag.dump();
    assert!(dump.contains("IfEqual") || dump.contains("ifequal") || dump.contains("Greater"), "{dump}");
}

#[test]
fn layers_and_fresnel() {
    let ir = lower(
        r#"mdl 1.7;
import ::df::*;
export material m() = material(surface: material_surface(scattering:
    df::fresnel_layer(ior: 1.5, layer: df::microfacet_ggx_smith_bsdf(roughness_u: 0.2), base: df::diffuse_reflection_bsdf(tint: color(0.8)))));
"#,
    );
    let closure = ir.dag.surface.closure.as_ref().unwrap().dump();
    assert!(closure.contains("dielectric_bsdf("), "{closure}");
    assert!(closure.contains("layer(top=c0, base=c1)"), "{closure}");
}
