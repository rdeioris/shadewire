use shadewire::interpret::{ContextSource, NoTextures, eval_surface};
use shadewire::optimize::optimize;
use shadewire::{ContextRead, Error, NodeKind, Op, Space, TransformKind, Type, Value};

use super::elaborate_output;
use crate::document::Document;
use crate::document::build::build;
use crate::parse::parse_str;

pub(super) fn doc(body: &str) -> Document {
    let text = format!(r#"<materialx version="1.39">{body}</materialx>"#);
    build(&parse_str(&text, None).unwrap(), Some("test.mtlx")).unwrap()
}

struct Zeros;
impl ContextSource for Zeros {
    fn read(&mut self, read: &ContextRead) -> Value {
        Value::zero(read.ty())
    }
    fn transform(&mut self, _: TransformKind, _: Space, _: Space, v: [f32; 3]) -> [f32; 3] {
        v
    }
}

fn eval_out(d: &Document, graph: &str) -> Value {
    let dag = elaborate_output(d, graph, "out").unwrap();
    eval_surface(&dag, &mut Zeros, &NoTextures).remove(0).1
}

#[test]
fn inlines_a_nodegraph_implemented_nodedef() {
    let d = doc(r#"
        <nodedef name="ND_double_float" node="double">
          <input name="in" type="float" value="0"/>
          <output name="out" type="float"/>
        </nodedef>
        <nodegraph name="NG_double_float" nodedef="ND_double_float">
          <multiply name="twice" type="float">
            <input name="in1" type="float" interfacename="in"/>
            <input name="in2" type="float" value="2"/>
          </multiply>
          <output name="out" type="float" nodename="twice"/>
        </nodegraph>
        <nodegraph name="g">
          <double name="d" type="float"><input name="in" type="float" value="3"/></double>
          <output name="out" type="float" nodename="d"/>
        </nodegraph>"#);
    let dag = optimize(&elaborate_output(&d, "g", "out").unwrap());
    let out = dag.node(dag.surface.fields[0].node);
    assert_eq!(out.kind, NodeKind::Const(Value::Float(6.0)));
}

#[test]
fn leaf_args_follow_the_catalog_order() {
    let d = doc(r#"<nodegraph name="g">
          <mix name="m" type="float">
            <input name="bg" type="float" value="0"/>
            <input name="mix" type="float" value="0.25"/>
            <input name="fg" type="float" value="1"/>
          </mix>
          <output name="out" type="float" nodename="m"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    let NodeKind::Op { op, args } = &dag.node(dag.surface.fields[0].node).kind else { panic!("not an op") };
    assert_eq!(*op, Op::Mix);
    let consts: Vec<&NodeKind> = args.iter().map(|a| &dag.node(*a).kind).collect();
    assert_eq!(
        consts,
        [
            &NodeKind::Const(Value::Float(1.0)),
            &NodeKind::Const(Value::Float(0.0)),
            &NodeKind::Const(Value::Float(0.25))
        ]
    );
    assert_eq!(eval_surface(&dag, &mut Zeros, &NoTextures)[0].1, Value::Float(0.25));
    assert_eq!(dag.node(dag.surface.fields[0].node).origin, "g/m");
}

#[test]
fn unconnected_texcoord_input_reads_uv0() {
    let d = doc(r#"<nodegraph name="g">
          <noise2d name="n" type="float"/>
          <output name="out" type="float" nodename="n"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    let uv0 = NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 });
    assert!(dag.nodes.iter().any(|n| n.kind == uv0), "{:#?}", dag.nodes);
}

#[test]
fn detects_cycles() {
    let d = doc(r#"<nodegraph name="g">
          <add name="a" type="float"><input name="in1" type="float" nodename="b"/></add>
          <add name="b" type="float"><input name="in1" type="float" nodename="a"/></add>
          <output name="out" type="float" nodename="a"/>
        </nodegraph>"#);
    let err = elaborate_output(&d, "g", "out").unwrap_err();
    let Error::Cycle { at, node } = err else { panic!("expected a cycle, got {err:?}") };
    assert_eq!(node, "a");
    assert_eq!(at.file.as_deref(), Some("test.mtlx"));
    assert_eq!(at.path, "g/a");
}

#[test]
fn interface_binds_through_a_user_nodegraph() {
    let d = doc(r#"<nodegraph name="inner">
          <input name="k" type="float" value="2"/>
          <add name="a" type="float">
            <input name="in1" type="float" interfacename="k"/>
            <input name="in2" type="float" value="1"/>
          </add>
          <output name="o" type="float" nodename="a"/>
        </nodegraph>
        <nodegraph name="g">
          <multiply name="m" type="float">
            <input name="in1" type="float" nodegraph="inner" output="o"/>
            <input name="in2" type="float" value="3"/>
          </multiply>
          <output name="out" type="float" nodename="m"/>
        </nodegraph>"#);
    assert_eq!(eval_out(&d, "g"), Value::Float(9.0));
}

#[test]
fn unbound_interfacename_falls_back_to_the_nodedef_default() {
    // ND_multiply_float declares in2 = 1.0; the graph's `k` has no value.
    let d = doc(r#"<nodegraph name="g">
          <input name="k" type="float"/>
          <multiply name="m" type="float">
            <input name="in1" type="float" value="5"/>
            <input name="in2" type="float" interfacename="k"/>
          </multiply>
          <output name="out" type="float" nodename="m"/>
        </nodegraph>"#);
    assert_eq!(eval_out(&d, "g"), Value::Float(5.0));
}

#[test]
fn extract_index_and_transform_spaces_become_op_params() {
    let d = doc(r#"<nodegraph name="g">
          <extract name="e" type="float">
            <input name="in" type="vector3" value="1,2,3"/>
            <input name="index" type="integer" value="2"/>
          </extract>
          <transformpoint name="t" type="vector3">
            <input name="fromspace" type="string" value="object"/>
            <input name="tospace" type="string" value="world"/>
          </transformpoint>
          <output name="out" type="float" nodename="e"/>
          <output name="moved" type="vector3" nodename="t"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    assert_eq!(eval_surface(&dag, &mut Zeros, &NoTextures)[0].1, Value::Float(3.0));
    let dag = elaborate_output(&d, "g", "moved").unwrap();
    let NodeKind::Op { op, .. } = &dag.node(dag.surface.fields[0].node).kind else { panic!("not an op") };
    assert_eq!(*op, Op::Transform { kind: TransformKind::Point, from: Space::Object, to: Space::World });
}

fn extract_error(source: &str, in_type: &str) -> String {
    let d = doc(&format!(
        r#"<nodegraph name="g">
          {source}
          <extract name="e" type="float">
            <input name="in" type="{in_type}" nodename="src"/>
            <input name="index" type="integer" value="7"/>
          </extract>
          <output name="out" type="float" nodename="e"/>
        </nodegraph>"#
    ));
    match elaborate_output(&d, "g", "out") {
        Err(Error::BadValue { message, .. }) => message,
        other => panic!("expected a bad value, got {other:?}"),
    }
}

#[test]
fn extract_index_past_the_last_component_is_a_bad_value() {
    let constant =
        r#"<constant name="src" type="color3"><input name="value" type="color3" value="1, 2, 3"/></constant>"#;
    assert_eq!(extract_error(constant, "color3"), "extract index 7 is out of range for color3");
    let texcoord = r#"<texcoord name="src" type="vector2"/>"#;
    assert_eq!(extract_error(texcoord, "vector2"), "extract index 7 is out of range for vector2");
}

#[test]
fn unknown_space_is_a_bad_value() {
    let d = doc(r#"<nodegraph name="g">
          <transformvector name="t" type="vector3">
            <input name="fromspace" type="string" value="camera"/>
            <input name="tospace" type="string" value="world"/>
          </transformvector>
          <output name="out" type="vector3" nodename="t"/>
        </nodegraph>"#);
    let err = elaborate_output(&d, "g", "out").unwrap_err();
    assert!(matches!(err, Error::BadValue { .. }), "{err:?}");
}

#[test]
fn matrix_nodes_are_unsupported() {
    let d = doc(r#"<nodegraph name="g">
          <add name="a" type="matrix33"/>
          <output name="out" type="matrix33" nodename="a"/>
        </nodegraph>"#);
    let err = elaborate_output(&d, "g", "out").unwrap_err();
    assert!(matches!(err, Error::UnsupportedType { .. }), "{err:?}");
}

#[test]
fn a_nodedef_without_graph_or_op_is_a_missing_leaf() {
    let d = doc(r#"<nodedef name="ND_mystery_float" node="mystery"><output name="out" type="float"/></nodedef>
        <nodegraph name="g">
          <mystery name="x" type="float"/>
          <output name="out" type="float" nodename="x"/>
        </nodegraph>"#);
    let err = elaborate_output(&d, "g", "out").unwrap_err();
    assert_eq!(err.to_string(), "test.mtlx:3 (g/x): no implementation for ND_mystery_float");
}

/// A node inside a library graph is reported in the library file, not the user's document.
#[test]
fn library_graph_errors_name_the_library_file() {
    let lib_text = r#"<materialx version="1.39">
        <nodedef name="ND_broken_float" node="broken"><output name="out" type="float"/></nodedef>
        <nodegraph name="NG_broken_float" nodedef="ND_broken_float">
          <bogus name="b" type="float"/>
          <output name="out" type="float" nodename="b"/>
        </nodegraph></materialx>"#;
    let lib_doc = build(&parse_str(lib_text, None).unwrap(), Some("lib/broken.mtlx")).unwrap();
    let library = crate::library::Library { documents: vec![lib_doc] };
    let d = doc(
        r#"<nodegraph name="g"><broken name="n" type="float"/><output name="out" type="float" nodename="n"/></nodegraph>"#,
    );
    let options = crate::IrOptions::default();
    let mut elaborator = super::Elaborator::new(&d, &library, &options, "test");
    let top = elaborator.top_scope(&d.nodes);
    let at = elaborator.at(&top, 0, "g".to_string());
    let scope = elaborator.user_graph_scope("g", &at).unwrap();
    let err = elaborator.graph_output(&scope, Some("out"), &at).unwrap_err();
    let Error::UnknownNode { at, category } = err else { panic!("{err:?}") };
    assert_eq!((at.file.as_deref(), at.line, category.as_str()), (Some("lib/broken.mtlx"), 4, "bogus"));
}
