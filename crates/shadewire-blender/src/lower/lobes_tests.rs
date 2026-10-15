use shadewire::{ClosureId, ClosureNode, Dag, Error, NodeKind, Value};
use shadewire_nodes::IrOptions;

use super::tests::{OUTPUT, PRINCIPLED, close};
use crate::to_ir;

fn lobes_options() -> IrOptions {
    IrOptions { values_as_uniforms: false, ..IrOptions::with_models(&["surface"]) }
}

/// A material whose Surface is `shader_node.output`; `extra` nodes come first.
fn closure_doc(extra: &str, shader_node: &str, output: &str, links: &str) -> String {
    let links: Vec<String> = serde_json::from_str::<Vec<[[String; 2]; 2]>>(links).unwrap().into_iter()
        .map(|[[a, b], [c, d]]| format!(r#"{{"from": ["{a}", "{b}"], "to": ["{c}", "{d}"], "muted": false, "valid": true}}"#))
        .chain(std::iter::once(format!(r#"{{"from": ["{shader_node}", "{output}"], "to": ["Material Output", "Surface"], "muted": false, "valid": true}}"#)))
        .collect();
    format!(
        r#"{{"format": "shadewire-blender-graph", "version": 1, "producer": "test", "groups": [], "images": [],
      "materials": [{{"name": "m", "settings": {{}}, "output": "Material Output", "links": [{}], "nodes": [{extra}, {OUTPUT}]}}]}}"#,
        links.join(", ")
    )
}

const DIFFUSE: &str = r#"{"name": "D", "type": "ShaderNodeBsdfDiffuse", "mute": false, "properties": {},
  "inputs": [{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true, "value": [0.5, 0.25, 0.125, 1.0]},
             {"identifier": "Roughness", "name": "Roughness", "type": "VALUE", "enabled": true, "value": 0.3},
             {"identifier": "Normal", "name": "Normal", "type": "VECTOR", "enabled": true, "value": [0, 0, 0]}],
  "outputs": [{"identifier": "BSDF", "name": "BSDF", "type": "SHADER", "enabled": true}], "internal_links": []}"#;
const EMISSION: &str = r#"{"name": "E", "type": "ShaderNodeEmission", "mute": false, "properties": {},
  "inputs": [{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true, "value": [1.0, 0.5, 0.0, 1.0]},
             {"identifier": "Strength", "name": "Strength", "type": "VALUE", "enabled": true, "value": 2.0}],
  "outputs": [{"identifier": "Emission", "name": "Emission", "type": "SHADER", "enabled": true}], "internal_links": []}"#;
const TRANSPARENT: &str = r#"{"name": "T", "type": "ShaderNodeBsdfTransparent", "mute": false, "properties": {},
  "inputs": [{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true, "value": [1.0, 1.0, 1.0, 1.0]}],
  "outputs": [{"identifier": "BSDF", "name": "BSDF", "type": "SHADER", "enabled": true}], "internal_links": []}"#;
const MIX: &str = r#"{"name": "Mix", "type": "ShaderNodeMixShader", "mute": false, "properties": {},
  "inputs": [{"identifier": "Fac", "name": "Fac", "type": "VALUE", "enabled": true, "value": 0.25},
             {"identifier": "Shader", "name": "Shader", "type": "SHADER", "enabled": true},
             {"identifier": "Shader_001", "name": "Shader", "type": "SHADER", "enabled": true}],
  "outputs": [{"identifier": "Shader", "name": "Shader", "type": "SHADER", "enabled": true}], "internal_links": []}"#;

fn lobe_kinds(dag: &Dag) -> Vec<String> {
    dag.surface
        .closure
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|n| match n {
            ClosureNode::Lobe { kind, .. } => Some(kind.clone()),
            _ => None,
        })
        .collect()
}

fn field(dag: &Dag, name: &str) -> Value {
    let f =
        dag.surface.fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no field {name}\n{}", dag.dump()));
    shadewire_nodes::elaborate::fold::fold_constant_of(dag, f.node).unwrap_or_else(|| panic!("{name} is not constant"))
}

#[test]
fn a_diffuse_bsdf_is_an_oren_nayar_lobe_and_needs_the_surface_model() {
    let text = closure_doc(DIFFUSE, "D", "BSDF", "[]");
    let ir = to_ir(&text, None, &lobes_options()).unwrap();
    assert_eq!(ir.dag.surface.model, "surface");
    assert_eq!(lobe_kinds(&ir.dag), ["oren_nayar_diffuse_bsdf"]);
    let closure = ir.dag.surface.closure.as_ref().unwrap();
    assert!(closure.bsdf.is_some() && closure.edf.is_none(), "{}", closure.dump());
    // The `surface` nodedef always has an opacity input: an opaque one is the field 1.
    let opacity = closure.opacity.as_ref().expect("a surface node always has an opacity field");
    assert!(close(&field(&ir.dag, opacity), &Value::Float(1.0)));
    let ClosureNode::Lobe { inputs, .. } = &closure.nodes[0] else { panic!() };
    assert!(close(&field(&ir.dag, &inputs["color"]), &Value::Color3([0.5, 0.25, 0.125])));
    assert!(matches!(to_ir(&text, None, &IrOptions::default()).unwrap_err(), Error::NoTranslation { .. }));
}

#[test]
fn mix_shader_mixes_lobes_and_emission_and_transparent_becomes_opacity() {
    let extra = format!("{DIFFUSE}, {EMISSION}, {MIX}");
    let text = closure_doc(
        &extra,
        "Mix",
        "Shader",
        r#"[[["D", "BSDF"], ["Mix", "Shader"]], [["E", "Emission"], ["Mix", "Shader_001"]]]"#,
    );
    let ir = to_ir(&text, None, &lobes_options()).unwrap();
    let closure = ir.dag.surface.closure.as_ref().unwrap();
    assert!(closure.bsdf.is_some() && closure.edf.is_some());
    // One-sided mixes weight the present side: the diffuse by 1 − 0.25, the emission by 0.25.
    let dump = closure.dump();
    assert_eq!(dump.matches("multiply").count(), 2, "{dump}");
    let edf = closure
        .nodes
        .iter()
        .find_map(|n| match n {
            ClosureNode::Lobe { kind, inputs, .. } if kind == "uniform_edf" => Some(inputs.clone()),
            _ => None,
        })
        .unwrap();
    assert!(close(&field(&ir.dag, &edf["color"]), &Value::Color3([2.0, 1.0, 0.0])));

    let extra = format!("{DIFFUSE}, {TRANSPARENT}, {MIX}");
    let text = closure_doc(
        &extra,
        "Mix",
        "Shader",
        r#"[[["D", "BSDF"], ["Mix", "Shader"]], [["T", "BSDF"], ["Mix", "Shader_001"]]]"#,
    );
    let ir = to_ir(&text, None, &lobes_options()).unwrap();
    let closure = ir.dag.surface.closure.as_ref().unwrap();
    let opacity = closure.opacity.as_ref().expect("transparency gives an opacity field");
    assert!(close(&field(&ir.dag, opacity), &Value::Float(0.75)), "{:?}", field(&ir.dag, opacity));
}

#[test]
fn a_principled_bsdf_mixed_with_transparent_keeps_the_model_and_scales_alpha() {
    let extra = format!("{PRINCIPLED}, {TRANSPARENT}, {MIX}");
    let text = closure_doc(
        &extra,
        "Mix",
        "Shader",
        r#"[[["Principled BSDF", "BSDF"], ["Mix", "Shader"]], [["T", "BSDF"], ["Mix", "Shader_001"]]]"#,
    );
    let options = IrOptions { values_as_uniforms: false, ..IrOptions::default() };
    let ir = to_ir(&text, None, &options).unwrap();
    assert_eq!(ir.dag.surface.model, "open_pbr_surface");
    assert!(close(&field(&ir.dag, "geometry_opacity"), &Value::Float(0.75)));
    assert!(ir.warnings.is_empty(), "{:?}", ir.warnings);
    let extra = format!("{PRINCIPLED}, {DIFFUSE}, {MIX}");
    let text = closure_doc(
        &extra,
        "Mix",
        "Shader",
        r#"[[["Principled BSDF", "BSDF"], ["Mix", "Shader"]], [["D", "BSDF"], ["Mix", "Shader_001"]]]"#,
    );
    let ir = to_ir(&text, None, &options).unwrap();
    assert_eq!(ir.dag.surface.model, "open_pbr_surface");
    assert_eq!(
        ir.warnings,
        [r#"ShaderNodeMixShader "Mix": Principled BSDF mixed with lobes, the Principled BSDF wins"#]
    );
}

#[test]
fn the_lobes_fixture_lowers_to_every_lobe_kind() {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../blender/fixtures/graphs/lobes.json"))
            .unwrap();
    let ir = to_ir(&text, None, &IrOptions::with_models(&["surface"])).unwrap();
    let mut kinds = lobe_kinds(&ir.dag);
    kinds.sort();
    kinds.dedup();
    for expected in [
        "oren_nayar_diffuse_bsdf",
        "generalized_schlick_bsdf",
        "dielectric_bsdf",
        "translucent_bsdf",
        "sheen_bsdf",
        "subsurface_bsdf",
        "conductor_bsdf",
        "uniform_edf",
    ] {
        assert!(kinds.contains(&expected.to_string()), "{expected} missing from {kinds:?}");
    }
    assert!(ir.dag.surface.closure.as_ref().unwrap().opacity.is_some());
    assert!(ir.warnings.iter().any(|w| w.contains("ShaderNodeBsdfAnisotropic")), "{:?}", ir.warnings);
    assert!(ir.dag.nodes.iter().any(|n| matches!(n.kind, NodeKind::Uniform { .. })));
}

const ADD: &str = r#"{"name": "Add", "type": "ShaderNodeAddShader", "mute": false, "properties": {},
  "inputs": [{"identifier": "Shader", "name": "Shader", "type": "SHADER", "enabled": true},
             {"identifier": "Shader_001", "name": "Shader", "type": "SHADER", "enabled": true}],
  "outputs": [{"identifier": "Shader", "name": "Shader", "type": "SHADER", "enabled": true}], "internal_links": []}"#;

#[test]
fn add_shader_adds_closures_and_a_shader_reached_twice_is_one_lobe() {
    let extra = format!("{DIFFUSE}, {ADD}");
    let links = r#"[[["D", "BSDF"], ["Add", "Shader"]], [["D", "BSDF"], ["Add", "Shader_001"]]]"#;
    let ir = to_ir(&closure_doc(&extra, "Add", "Shader", links), None, &lobes_options()).unwrap();
    assert_eq!(lobe_kinds(&ir.dag), ["oren_nayar_diffuse_bsdf"]);
    let dump = ir.dag.surface.closure.as_ref().unwrap().dump();
    assert!(dump.contains("add(c0, c0)"), "{dump}");

    // Two white transparent shaders add to a transparency of 2, capped at 1: the surface is invisible.
    let extra = format!("{TRANSPARENT}, {ADD}");
    let links = r#"[[["T", "BSDF"], ["Add", "Shader"]], [["T", "BSDF"], ["Add", "Shader_001"]]]"#;
    let ir = to_ir(&closure_doc(&extra, "Add", "Shader", links), None, &lobes_options()).unwrap();
    let closure = ir.dag.surface.closure.as_ref().unwrap();
    assert!(closure.bsdf.is_none() && closure.edf.is_none());
    assert!(close(&field(&ir.dag, closure.opacity.as_ref().unwrap()), &Value::Float(0.0)));
}

/// Each lobe's kind and the product of the weights on its path from the BSDF or the EDF root, sorted by kind.
fn weights(dag: &Dag) -> Vec<(String, f32)> {
    let closure = dag.surface.closure.as_ref().unwrap();
    let mut stack: Vec<(ClosureId, f32)> = closure.bsdf.into_iter().chain(closure.edf).map(|id| (id, 1.0)).collect();
    let mut found = Vec::new();
    while let Some((id, w)) = stack.pop() {
        match &closure.nodes[id as usize] {
            ClosureNode::Lobe { kind, .. } => found.push((kind.clone(), w)),
            ClosureNode::Mix { fg, bg, mix } => {
                let f = field(dag, mix).floats()[0];
                stack.push((*bg, w * (1.0 - f)));
                stack.push((*fg, w * f));
            }
            ClosureNode::Add { in1, in2 } => stack.extend([(*in1, w), (*in2, w)]),
            ClosureNode::Multiply { in1, in2 } => stack.push((*in1, w * field(dag, in2).floats()[0])),
            ClosureNode::Layer { .. } => panic!("no layer here:\n{}", closure.dump()),
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// The surface's opacity, folded; 1 when it has none.
fn opacity(dag: &Dag) -> f32 {
    let closure = dag.surface.closure.as_ref().unwrap();
    closure.opacity.as_ref().map_or(1.0, |name| field(dag, name).floats()[0])
}

fn assert_weights(dag: &Dag, expected: &[(&str, f32)]) {
    let found = weights(dag);
    let same = found.len() == expected.len()
        && found.iter().zip(expected).all(|((k, w), (ek, ew))| k == ek && (w - ew).abs() <= 1e-4);
    assert!(same, "{found:?} against {expected:?}\n{}", dag.surface.closure.as_ref().unwrap().dump());
}

#[test]
fn transparency_passes_light_and_every_lobe_keeps_its_blender_weight() {
    let lower = |extra: &str, surface: &str, links: &str| {
        to_ir(&closure_doc(extra, surface, "Shader", links), None, &lobes_options()).unwrap().dag
    };
    // Mix(0.25, D, T) renders 0.75 D + 0.25 behind: the opacity is 0.75, D weighted 0.75 then divided by it.
    let extra = format!("{DIFFUSE}, {TRANSPARENT}, {MIX}");
    let dag = lower(&extra, "Mix", r#"[[["D", "BSDF"], ["Mix", "Shader"]], [["T", "BSDF"], ["Mix", "Shader_001"]]]"#);
    assert!((opacity(&dag) - 0.75).abs() <= 1e-4, "{}", opacity(&dag));
    assert_weights(&dag, &[("oren_nayar_diffuse_bsdf", 1.0)]);
    // The transparent shader first: 0.25 D over an opacity of 0.25.
    let dag = lower(&extra, "Mix", r#"[[["T", "BSDF"], ["Mix", "Shader"]], [["D", "BSDF"], ["Mix", "Shader_001"]]]"#);
    assert!((opacity(&dag) - 0.25).abs() <= 1e-4, "{}", opacity(&dag));
    assert_weights(&dag, &[("oren_nayar_diffuse_bsdf", 1.0)]);
    // Add(D, Mix(0.5, E, T)) renders D + 0.5 E + 0.5 behind: over an opacity of 0.5, D is 2 and E is 1.
    let half = MIX.replace(r#""Mix""#, r#""Half""#).replace("0.25", "0.5");
    let extra = format!("{DIFFUSE}, {EMISSION}, {TRANSPARENT}, {half}, {ADD}");
    let links = r#"[[["D", "BSDF"], ["Add", "Shader"]], [["Half", "Shader"], ["Add", "Shader_001"]],
      [["E", "Emission"], ["Half", "Shader"]], [["T", "BSDF"], ["Half", "Shader_001"]]]"#;
    let dag = lower(&extra, "Add", links);
    assert!((opacity(&dag) - 0.5).abs() <= 1e-4, "{}", opacity(&dag));
    assert_weights(&dag, &[("oren_nayar_diffuse_bsdf", 2.0), ("uniform_edf", 1.0)]);
    // An unlinked shader renders black: the energy is lost, D is weighted 0.75 and nothing is divided.
    let extra = format!("{DIFFUSE}, {MIX}");
    let dag = lower(&extra, "Mix", r#"[[["D", "BSDF"], ["Mix", "Shader"]]]"#);
    assert_eq!(opacity(&dag), 1.0);
    assert_weights(&dag, &[("oren_nayar_diffuse_bsdf", 0.75)]);
    let dump = dag.surface.closure.as_ref().unwrap().dump();
    assert_eq!(dump.matches("multiply").count(), 1, "{dump}");
}

#[test]
fn a_surface_linked_to_a_non_surface_shader_names_it_in_the_error() {
    let volume = r#"{"name": "VP", "type": "ShaderNodeVolumePrincipled", "mute": false, "properties": {}, "inputs": [],
      "outputs": [{"identifier": "Volume", "name": "Volume", "type": "SHADER", "enabled": true}], "internal_links": []}"#;
    let text = closure_doc(volume, "VP", "Volume", "[]");
    let err = to_ir(&text, None, &lobes_options()).unwrap_err();
    assert!(matches!(err, Error::UnsupportedSurface { .. }), "{err}");
    assert!(err.to_string().contains("ShaderNodeVolumePrincipled"), "{err}");
    assert!(!err.to_string().contains("not linked"), "{err}");
}
