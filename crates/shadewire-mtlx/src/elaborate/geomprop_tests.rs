//! An unconnected input whose nodedef port has a `defaultgeomprop` reads the geometric property even when
//! the node authors a value (MaterialX ShaderGen, `ShaderGraph::createNode`); a connection still wins.

use shadewire::{ContextRead, Dag, NodeKind, Space};

use crate::{MtlxOptions, to_ir};

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
    to_ir(&text, None, None, &MtlxOptions::default()).unwrap().dag
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
