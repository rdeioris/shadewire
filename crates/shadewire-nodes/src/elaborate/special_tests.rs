//! Special categories: constant, dot, convert, separate, geometric reads.

use std::collections::HashMap;

use shadewire::{ContextRead, Dag, Error, NodeKind, Op, Space, Type, Value};

use super::tests::doc;
use super::{Elaborator, elaborate_output};
use crate::document::Document;
use crate::options::LowerOptions;

/// The kind of the node graph `g`'s output `out` elaborates to.
fn out_kind(d: &Document) -> NodeKind {
    let dag = elaborate_output(d, "g", "out").unwrap();
    dag.node(dag.surface.fields[0].node).kind.clone()
}

/// A single-node graph `g` whose output `out` (typed `ty`) reads node `n`.
fn one(node: &str, ty: &str) -> Document {
    doc(&format!(r#"<nodegraph name="g">{node}<output name="out" type="{ty}" nodename="n"/></nodegraph>"#))
}

fn context(read: ContextRead) -> NodeKind {
    NodeKind::Context(read)
}

#[test]
fn constant_and_dot_pass_their_input_through() {
    let d = doc(r#"<nodegraph name="g">
          <constant name="c" type="color3"><input name="value" type="color3" value="0.1,0.2,0.3"/></constant>
          <dot name="d" type="color3"><input name="in" type="color3" nodename="c"/></dot>
          <output name="out" type="color3" nodename="d"/>
        </nodegraph>"#);
    assert_eq!(out_kind(&d), NodeKind::Const(Value::Color3([0.1, 0.2, 0.3])));
}

#[test]
fn convert_without_a_graph_is_a_convert_op() {
    let d = one(r#"<convert name="n" type="float"><input name="in" type="integer" value="3"/></convert>"#, "float");
    let NodeKind::Op { op, args } = out_kind(&d) else { panic!("not an op") };
    assert_eq!(op, Op::Convert);
    assert_eq!(args.len(), 1);
}

#[test]
fn separate_extracts_the_position_of_the_requested_output() {
    let d = doc(r#"<nodedef name="ND_sep3_mine" node="separate3">
          <input name="in" type="color3" value="0,0,0"/>
          <output name="outr" type="float"/><output name="outg" type="float"/><output name="outb" type="float"/>
        </nodedef>
        <nodedef name="ND_sep4_mine" node="separate4">
          <input name="in" type="vector4" value="0,0,0,0"/>
          <output name="outx" type="float"/><output name="outy" type="float"/>
          <output name="outz" type="float"/><output name="outw" type="float"/>
        </nodedef>
        <nodegraph name="g">
          <separate3 name="s" type="multioutput" nodedef="ND_sep3_mine"/>
          <separate4 name="t" type="multioutput" nodedef="ND_sep4_mine"/>
          <output name="out" type="float" nodename="s" output="outg"/>
          <output name="w" type="float" nodename="t" output="outw"/>
        </nodegraph>"#);
    let NodeKind::Op { op, .. } = out_kind(&d) else { panic!("not an op") };
    assert_eq!(op, Op::Extract(1));
    let dag = elaborate_output(&d, "g", "w").unwrap();
    let NodeKind::Op { op, .. } = &dag.node(dag.surface.fields[0].node).kind else { panic!("not an op") };
    assert_eq!(*op, Op::Extract(3));
}

#[test]
fn geometric_nodes_read_their_space_with_defaults() {
    let p = one(
        r#"<position name="n" type="vector3"><input name="space" type="string" value="world"/></position>"#,
        "vector3",
    );
    assert_eq!(out_kind(&p), context(ContextRead::Position(Space::World)));
    let n = one(r#"<normal name="n" type="vector3"/>"#, "vector3");
    assert_eq!(out_kind(&n), context(ContextRead::Normal(Space::Object)));
    let t = one(
        r#"<tangent name="n" type="vector3"><input name="space" type="string" value="model"/></tangent>"#,
        "vector3",
    );
    assert_eq!(out_kind(&t), context(ContextRead::Tangent(Space::Model)));
    let b = one(r#"<bitangent name="n" type="vector3"/>"#, "vector3");
    assert_eq!(out_kind(&b), context(ContextRead::Bitangent(Space::Object)));
    // Not in the vendored 1.39 stdlib; a nodedef with no space value reads world.
    let v = doc(r#"<nodedef name="ND_viewdirection_vector3" node="viewdirection">
          <input name="space" type="string"/><output name="out" type="vector3"/>
        </nodedef>
        <nodegraph name="g"><viewdirection name="n" type="vector3"/><output name="out" type="vector3" nodename="n"/></nodegraph>"#);
    assert_eq!(out_kind(&v), context(ContextRead::ViewDirection(Space::World)));
}

#[test]
fn unknown_space_on_a_geometric_node_is_a_bad_value() {
    let p = one(
        r#"<position name="n" type="vector3"><input name="space" type="string" value="camera"/></position>"#,
        "vector3",
    );
    let err = elaborate_output(&p, "g", "out").unwrap_err();
    assert!(matches!(err, Error::BadValue { .. }), "{err:?}");
}

#[test]
fn texcoord_and_geomcolor_read_their_index_and_type() {
    let t = one(
        r#"<texcoord name="n" type="vector3"><input name="index" type="integer" value="1"/></texcoord>"#,
        "vector3",
    );
    assert_eq!(out_kind(&t), context(ContextRead::TexCoord { index: 1, ty: Type::Vector3 }));
    let t0 = one(r#"<texcoord name="n" type="vector2"/>"#, "vector2");
    assert_eq!(out_kind(&t0), context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }));
    let c = one(
        r#"<geomcolor name="n" type="color4"><input name="index" type="integer" value="2"/></geomcolor>"#,
        "color4",
    );
    assert_eq!(out_kind(&c), context(ContextRead::GeomColor { index: 2, ty: Type::Color4 }));
}

#[test]
fn time_and_frame_are_global_reads() {
    let t = one(r#"<time name="n" type="float"/>"#, "float");
    assert_eq!(out_kind(&t), context(ContextRead::Time));
    let f = one(r#"<frame name="n" type="float"/>"#, "float");
    assert_eq!(out_kind(&f), context(ContextRead::Frame));
}

/// The DAG and the recorded geomprop defaults for graph `g`'s output `out`.
fn with_defaults(d: &Document) -> (Dag, HashMap<String, Value>) {
    let options = LowerOptions::default();
    let mut elaborator = Elaborator::new(d, crate::library::standard(), &options, "test");
    let top = elaborator.top_scope(&d.nodes);
    let at = elaborator.at(&top, 0, "g".to_string());
    let scope = elaborator.user_graph_scope("g", &at).unwrap();
    let id = elaborator.graph_output(&scope, Some("out"), &at).unwrap();
    let ty = elaborator.dag.node(id).ty;
    elaborator.dag.surface.fields.push(shadewire::SurfaceField { name: "out".to_string(), ty, node: id });
    (elaborator.dag, elaborator.geomprop_defaults)
}

#[test]
fn geompropvalue_reads_the_property_and_records_its_default() {
    let d = one(
        r#"<geompropvalue name="n" type="float">
          <input name="geomprop" type="string" value="wear"/>
          <input name="default" type="float" value="0.5"/>
        </geompropvalue>"#,
        "float",
    );
    let (dag, defaults) = with_defaults(&d);
    let read = ContextRead::GeomProp { name: "wear".to_string(), ty: Type::Float };
    assert_eq!(dag.node(dag.surface.fields[0].node).kind, context(read));
    assert_eq!(defaults.get("wear"), Some(&Value::Float(0.5)));
}

#[test]
fn geompropvalue_without_a_name_is_a_bad_value() {
    let d = one(r#"<geompropvalue name="n" type="float"/>"#, "float");
    let err = elaborate_output(&d, "g", "out").unwrap_err();
    assert!(matches!(err, Error::BadValue { .. }), "{err:?}");
}
