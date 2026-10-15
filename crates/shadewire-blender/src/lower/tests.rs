//! A document around one node under test: the node, a Principled BSDF it feeds, a Material Output.

use shadewire::interpret::{NoTextures, eval_surface};
use shadewire::testing::{Probe, ProbeTextures};
use shadewire::{Dag, Value};
use shadewire_nodes::IrOptions;
use shadewire_nodes::elaborate::fold::fold_constant_of;

use crate::to_ir;

/// A one-material document: `nodes` (JSON objects, the node under test first) feed the Principled BSDF through
/// `links` (`[["node", "output"], ["Principled BSDF", "input"]]` pairs). Hand-written: sockets not named are absent.
pub fn doc(nodes: &str, links: &str) -> String {
    let links: Vec<(String, String, String, String)> = serde_json::from_str::<Vec<[[String; 2]; 2]>>(links)
        .unwrap()
        .into_iter()
        .map(|[[a, b], [c, d]]| (a, b, c, d))
        .collect();
    let links: Vec<String> = links
        .iter()
        .map(|(a, b, c, d)| format!(r#"{{"from": ["{a}", "{b}"], "to": ["{c}", "{d}"], "muted": false, "valid": true}}"#))
        .chain(std::iter::once(
            r#"{"from": ["Principled BSDF", "BSDF"], "to": ["Material Output", "Surface"], "muted": false, "valid": true}"#
                .to_string(),
        ))
        .collect();
    format!(
        r#"{{"format": "shadewire-blender-graph", "version": 1, "producer": "test", "groups": [], "images": [],
  "materials": [{{"name": "m", "settings": {{}}, "output": "Material Output", "links": [{}],
    "nodes": [{nodes}, {PRINCIPLED}, {OUTPUT}]}}]}}"#,
        links.join(", ")
    )
}

pub const PRINCIPLED: &str = r#"{"name": "Principled BSDF", "type": "ShaderNodeBsdfPrincipled", "mute": false, "properties": {},
  "inputs": [
    {"identifier": "Base Color", "name": "Base Color", "type": "RGBA", "enabled": true, "value": [0.8, 0.8, 0.8, 1.0]},
    {"identifier": "Metallic", "name": "Metallic", "type": "VALUE", "enabled": true, "value": 0.0},
    {"identifier": "Roughness", "name": "Roughness", "type": "VALUE", "enabled": true, "value": 0.5},
    {"identifier": "IOR", "name": "IOR", "type": "VALUE", "enabled": true, "value": 1.5},
    {"identifier": "Alpha", "name": "Alpha", "type": "VALUE", "enabled": true, "value": 1.0},
    {"identifier": "Normal", "name": "Normal", "type": "VECTOR", "enabled": true, "value": [0.0, 0.0, 0.0]},
    {"identifier": "Emission Color", "name": "Emission Color", "type": "RGBA", "enabled": true, "value": [1.0, 1.0, 1.0, 1.0]},
    {"identifier": "Emission Strength", "name": "Emission Strength", "type": "VALUE", "enabled": true, "value": 0.0},
    {"identifier": "Subsurface Radius", "name": "Subsurface Radius", "type": "VECTOR", "enabled": true, "value": [1.0, 0.2, 0.1]}],
  "outputs": [{"identifier": "BSDF", "name": "BSDF", "type": "SHADER", "enabled": true}], "internal_links": []}"#;

pub const OUTPUT: &str = r#"{"name": "Material Output", "type": "ShaderNodeOutputMaterial", "mute": false,
  "properties": {"is_active_output": true, "target": "ALL"},
  "inputs": [{"identifier": "Surface", "name": "Surface", "type": "SHADER", "enabled": true},
             {"identifier": "Volume", "name": "Volume", "type": "SHADER", "enabled": true},
             {"identifier": "Displacement", "name": "Displacement", "type": "VECTOR", "enabled": true, "value": [0.0, 0.0, 0.0]}],
  "outputs": [], "internal_links": []}"#;

/// The dag of `doc(nodes, links)` with values as constants, and the warnings.
pub fn lower(nodes: &str, links: &str) -> (Dag, Vec<String>) {
    let options = IrOptions { values_as_uniforms: false, ..IrOptions::default() };
    let ir = to_ir(&doc(nodes, links), None, &options).unwrap_or_else(|e| panic!("{e}"));
    (ir.dag, ir.warnings)
}

/// The constant value of `field` after lowering `nodes` into the Principled BSDF: the node under test's math over
/// constant inputs, folded. Panics when the field is not a constant (a context read or a texture reached it).
pub fn lower_field(nodes: &str, links: &str, field: &str) -> (Value, Vec<String>) {
    let (dag, warnings) = lower(nodes, links);
    let f = dag.surface.fields.iter().find(|f| f.name == field).unwrap_or_else(|| panic!("no field {field}"));
    let value = fold_constant_of(&dag, f.node).unwrap_or_else(|| panic!("{field} is not a constant:\n{}", dag.dump()));
    (value, warnings)
}

/// `field` evaluated at probe `k` (context reads answered by `Probe`, textures by `ProbeTextures`).
#[allow(dead_code)] // the handler tests of later tasks evaluate fields that read the context
pub fn eval_field(dag: &Dag, field: &str, k: u32) -> Value {
    let values = eval_surface(dag, &mut Probe(k), &ProbeTextures);
    values.into_iter().find(|(n, _)| n == field).map(|(_, v)| v).unwrap_or_else(|| panic!("no field {field}"))
}

pub fn close(a: &Value, b: &Value) -> bool {
    a.floats().iter().zip(b.floats()).all(|(x, y)| (x - y).abs() <= 1e-4)
}

#[allow(dead_code)]
pub fn no_textures() -> NoTextures {
    NoTextures
}
