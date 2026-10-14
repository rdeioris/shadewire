//! An unconnected input whose nodedef port has a `defaultgeomprop` reads the geometric property even when
//! the node authors a value (MaterialX ShaderGen, `ShaderGraph::createNode`); a connection still wins.

use shadewire::{ContextRead, Dag, NodeKind, Space, Type};

use crate::document::build::build;
use crate::library::standard;
use crate::parse::parse_str;
use crate::{IrOptions, elaborate};

fn to_dag(text: &str) -> Dag {
    let document = build(&parse_str(text, None).unwrap(), None).unwrap();
    elaborate(&document, standard(), &IrOptions::default(), None).unwrap()
}

fn gltf(normal: &str) -> Dag {
    let text = format!(
        r#"<materialx version="1.39">
  <constant name="up" type="vector3"><input name="value" type="vector3" value="0, 1, 0"/></constant>
  <gltf_pbr name="SR" type="surfaceshader">
    <input name="base_color" type="color3" value="0.5, 0.5, 0.5" />
    {normal}
  </gltf_pbr>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR" />
  </surfacematerial>
</materialx>"#
    );
    to_dag(&text)
}

fn field<'d>(dag: &'d Dag, name: &str) -> &'d NodeKind {
    let field = dag.surface.fields.iter().find(|f| f.name == name).unwrap();
    &dag.nodes[field.node.index()].kind
}

#[test]
fn an_authored_value_on_a_geomprop_input_reads_the_geomprop() {
    let dag = gltf(r#"<input name="normal" type="vector3" value="0, 0, 1" />"#);
    assert!(matches!(field(&dag, "normal"), NodeKind::Context(ContextRead::Normal(Space::World))), "{dag:?}");
    assert!(matches!(field(&dag, "base_color"), NodeKind::Uniform { .. }));
}

#[test]
fn a_connected_geomprop_input_keeps_its_connection() {
    let dag = gltf(r#"<input name="normal" type="vector3" nodename="up" />"#);
    assert!(!matches!(field(&dag, "normal"), NodeKind::Context(_)), "{:?}", field(&dag, "normal"));
}

/// A user nodegraph's interface input with both a value and a `defaultgeomprop` reads the geomprop.
#[test]
fn a_graph_interface_input_with_a_value_reads_its_geomprop() {
    let text = r#"<materialx version="1.39">
  <nodegraph name="NG">
    <input name="n" type="vector3" value="0, 0, 1" defaultgeomprop="Nworld" />
    <normalize name="norm" type="vector3"><input name="in" type="vector3" interfacename="n" /></normalize>
    <output name="out" type="vector3" nodename="norm" />
  </nodegraph>
  <gltf_pbr name="SR" type="surfaceshader">
    <input name="normal" type="vector3" nodegraph="NG" output="out" />
  </gltf_pbr>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR" />
  </surfacematerial>
</materialx>"#;
    let dag = to_dag(text);
    let reads_normal = dag.nodes.iter().any(|n| matches!(n.kind, NodeKind::Context(ContextRead::Normal(Space::World))));
    let has_uniform = dag.nodes.iter().any(|n| matches!(n.kind, NodeKind::Uniform { .. }) && n.ty == Type::Vector3);
    assert!(reads_normal && !has_uniform, "{}", dag.dump());
}
