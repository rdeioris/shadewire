//! Converted values read as parameters fold to constants; converted
//! uniforms keep their conversion; unit scaling of non-constant values.

use shadewire::interpret::{ContextSource, NoTextures, eval_surface};
use shadewire::{ContextRead, Dag, Location, Node, NodeKind, Op, Space, TransformKind, Type, Value};

use super::{ElaborateOptions, Elaborator, elaborate_output};
use crate::document::Document;
use crate::document::build::build;
use crate::parse::parse_str;

/// A document whose authored colors are in `srgb_texture`.
fn srgb_doc(body: &str) -> Document {
    let text = format!(r#"<materialx version="1.39" colorspace="srgb_texture">{body}</materialx>"#);
    build(&parse_str(&text, None).unwrap(), Some("test.mtlx")).unwrap()
}

fn assert_close(got: &[f32], want: f32) {
    for g in got {
        assert!((g - want).abs() < 1e-4, "got {got:?}, want {want}");
    }
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

#[test]
fn image_color_default_in_a_colorspace_folds_to_a_constant() {
    let d = srgb_doc(
        r#"<nodegraph name="g">
          <image name="i" type="color3">
            <input name="file" type="filename" value="a.png"/>
            <input name="default" type="color3" value="0.5,0.5,0.5"/>
          </image>
          <output name="out" type="color3" nodename="i"/>
        </nodegraph>"#,
    );
    let dag = elaborate_output(&d, "g", "out").unwrap();
    let default = dag.nodes.iter().find_map(|n| match &n.kind {
        NodeKind::Sample { default, .. } => Some(default.clone()),
        _ => None,
    });
    let Some(Value::Color3(c)) = default else { panic!("no color3 sample default: {default:?}") };
    assert_close(&c, 0.2140);
}

#[test]
fn geompropvalue_color_default_in_a_colorspace_folds_to_a_constant() {
    let d = srgb_doc(
        r#"<nodegraph name="g">
          <geompropvalue name="p" type="color3">
            <input name="geomprop" type="string" value="tint"/>
            <input name="default" type="color3" value="0.5,0.5,0.5"/>
          </geompropvalue>
          <output name="out" type="color3" nodename="p"/>
        </nodegraph>"#,
    );
    let options = ElaborateOptions::default();
    let mut e = Elaborator::new(&d, crate::library::standard(), &options, "test");
    let top = e.top_scope(&d.nodes);
    let at = e.at(&top, 0, "g".to_string());
    let scope = e.user_graph_scope("g", &at).unwrap();
    e.graph_output(&scope, Some("out"), &at).unwrap();
    let Some(Value::Color3(c)) = e.geomprop_defaults.get("tint") else { panic!("{:?}", e.geomprop_defaults) };
    assert_close(c, 0.2140);
}

#[test]
fn interface_uniform_in_a_colorspace_keeps_its_conversion() {
    let d = srgb_doc(
        r#"<nodegraph name="g">
          <input name="tint" type="color3" value="0.5,0.5,0.5"/>
          <dot name="d" type="color3"><input name="in" type="color3" interfacename="tint"/></dot>
          <output name="out" type="color3" nodename="d"/>
        </nodegraph>"#,
    );
    let dag: Dag = elaborate_output(&d, "g", "out").unwrap();
    let out = dag.node(dag.surface.fields[0].node);
    assert!(matches!(out.kind, NodeKind::Op { .. }), "not converted: {:?}", out.kind);
    assert!(dag.nodes.iter().any(|n| matches!(&n.kind, NodeKind::Uniform { name, .. } if name == "tint")));
    let Value::Color3(c) = eval_surface(&dag, &mut Zeros, &NoTextures).remove(0).1 else { panic!() };
    assert_close(&c, 0.2140);
}

#[test]
fn distance_on_a_non_constant_value_multiplies() {
    let d = srgb_doc("");
    let options = ElaborateOptions::default();
    let mut e = Elaborator::new(&d, crate::library::standard(), &options, "test");
    let read = NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 });
    let uv = e.dag.add(Node { kind: read, ty: Type::Vector2, origin: String::new() });
    let at = Location { file: None, line: 0, path: "x".to_string() };
    let id = e.convert_unit(uv, Type::Vector2, Some("centimeter"), Some("distance"), Some("meter"), &at).unwrap();
    let NodeKind::Op { op: Op::Multiply, args } = &e.dag.node(id).kind else { panic!("{:?}", e.dag.node(id)) };
    assert_eq!(args[0], uv);
    let NodeKind::Const(Value::Vector2(f)) = &e.dag.node(args[1]).kind else { panic!("{:?}", e.dag.node(args[1])) };
    assert_close(f, 0.01);
}
