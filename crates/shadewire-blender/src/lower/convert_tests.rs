use shadewire::{Type, Value};
use shadewire_nodes::elaborate::fold::fold_constant_of;
use shadewire_nodes::{Elaborator, IrOptions, Synthetic};

use super::Lowerer;
use super::tests::{doc, lower_field};
use crate::document::parse;
use crate::graph::Graph;

fn value(x: f32) -> String {
    format!(
        r#"{{"name": "V", "type": "ShaderNodeValue", "mute": false, "properties": {{}}, "inputs": [],
          "outputs": [{{"identifier": "Value", "name": "Value", "type": "VALUE", "enabled": true, "value": {x}}}],
          "internal_links": []}}"#
    )
}

#[test]
fn a_float_linked_into_a_boolean_socket_is_true_when_greater_than_zero() {
    let links = r#"[[["V", "Value"], ["Principled BSDF", "Thin Wall"]]]"#;
    let (on, warnings) = lower_field(&value(0.25), links, "geometry_thin_walled");
    assert_eq!(on, Value::Boolean(true));
    assert!(warnings.is_empty(), "{warnings:?}");
    let (off, _) = lower_field(&value(0.0), links, "geometry_thin_walled");
    assert_eq!(off, Value::Boolean(false));
}

#[test]
fn a_float_linked_into_an_integer_socket_is_floored_with_a_warning() {
    let reader = r#"{"name": "I", "type": "ShaderNodeFake", "mute": false, "properties": {},
      "inputs": [{"identifier": "Count", "name": "Count", "type": "INT", "enabled": true, "value": 0}],
      "outputs": [], "internal_links": []}"#;
    let text = doc(&format!("{}, {reader}", value(2.5)), r#"[[["V", "Value"], ["I", "Count"]]]"#);
    let doc = parse(&text).unwrap();
    let (graph, _) = Graph::flatten(&doc, &doc.materials[0]).unwrap();
    let (synthetic, options) = (Synthetic::new(), IrOptions { values_as_uniforms: false, ..IrOptions::default() });
    let mut l = Lowerer::new(Elaborator::standalone(&synthetic, &options), &graph, "m", &[]);
    let id = l.input(graph.node("I").unwrap(), "Count", Type::Integer).unwrap();
    assert_eq!(fold_constant_of(&l.e.dag, id), Some(Value::Integer(2)));
    assert_eq!(l.warnings, [r#"ShaderNodeFake "I": Count read as an integer by floor"#]);
}
