use shadewire::{AddressMode, ContextRead, Error, NodeKind, Type, Value};

use super::elaborate_output;
use super::tests::doc;
use crate::elaborate::{Arg, Elaborator};
use crate::options::IrOptions;
use crate::synthetic::Synthetic;

fn standalone<'a>(synthetic: &'a Synthetic, options: &'a IrOptions) -> Elaborator<'a> {
    Elaborator::standalone(synthetic, options)
}

#[test]
fn a_leaf_node_over_values() {
    let (s, o) = (Synthetic::new(), IrOptions::default());
    let mut e = standalone(&s, &o);
    let id = e
        .instantiate(
            "add",
            Type::Float,
            &[("in1", Arg::Value(Value::Float(1.0))), ("in2", Arg::Value(Value::Float(2.0)))],
            "sum",
        )
        .unwrap();
    assert_eq!(e.fold_constant(id), Some(Value::Float(3.0)));
    assert_eq!(e.dag.node(id).origin, "sum");
    assert!(e.dag.compounds.is_empty());
}

#[test]
fn a_graph_node_is_inlined_and_recorded_as_a_compound() {
    let (s, o) = (Synthetic::new(), IrOptions::default());
    let mut e = standalone(&s, &o);
    let fg = e.constant(Value::Color3([0.2, 0.4, 0.6]), "fg");
    let id = e
        .instantiate(
            "overlay",
            Type::Color3,
            &[("fg", Arg::Node(fg)), ("bg", Arg::Value(Value::Color3([0.5, 0.5, 0.5])))],
            "o",
        )
        .unwrap();
    let d = doc(r#"
        <nodegraph name="g">
          <overlay name="o" type="color3">
            <input name="fg" type="color3" value="0.2, 0.4, 0.6"/>
            <input name="bg" type="color3" value="0.5, 0.5, 0.5"/>
          </overlay>
          <output name="out" type="color3" nodename="o"/>
        </nodegraph>"#);
    let from_document = elaborate_output(&d, "g", "out").unwrap();
    let expected = crate::elaborate::fold::fold_constant_of(&from_document, from_document.surface.fields[0].node);
    assert!(expected.is_some());
    assert_eq!(e.fold_constant(id), expected);
    // Library nodes inlined inside the overlay are compounds too, so find the instance by its origin.
    let c = e.dag.compounds.iter().find(|c| c.origin == "o").expect("the overlay instance is recorded");
    assert_eq!((c.nodedef.as_str(), c.origin.as_str()), ("ND_overlay_color3", "o"));
    assert_eq!(c.inputs[0], ("fg".to_string(), fg));
}

#[test]
fn a_reused_origin_keeps_two_compounds() {
    let (s, o) = (Synthetic::new(), IrOptions::default());
    let mut e = standalone(&s, &o);
    let mut mix = |fg: [f32; 3]| {
        e.instantiate(
            "overlay",
            Type::Color3,
            &[("fg", Arg::Value(Value::Color3(fg))), ("bg", Arg::Value(Value::Color3([0.5, 0.5, 0.5])))],
            "Mix",
        )
        .unwrap()
    };
    let first = mix([0.2, 0.4, 0.6]);
    let second = mix([0.7, 0.1, 0.3]);
    let compounds: Vec<_> = e.dag.compounds.iter().filter(|c| c.nodedef == "ND_overlay_color3").collect();
    assert_eq!(compounds.len(), 2);
    assert_eq!((compounds[0].origin.as_str(), compounds[0].outputs.clone()), ("Mix", vec![("out".to_string(), first)]));
    assert_eq!(
        (compounds[1].origin.as_str(), compounds[1].outputs.clone()),
        ("Mix_2", vec![("out".to_string(), second)])
    );
    assert_ne!(first, second);
}

#[test]
fn an_image_takes_a_file_with_its_colorspace_and_address_modes() {
    let (s, o) = (Synthetic::new(), IrOptions::default());
    let mut e = standalone(&s, &o);
    let uv = e.context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }, "uv");
    let id = e
        .instantiate(
            "image",
            Type::Color3,
            &[
                ("file", Arg::File { path: "a.png".into(), colorspace: Some("srgb_texture".into()) }),
                ("texcoord", Arg::Node(uv)),
                ("uaddressmode", Arg::Text("clamp".into())),
            ],
            "img",
        )
        .unwrap();
    assert_eq!(e.dag.textures.len(), 1);
    assert_eq!(e.dag.textures[0].file, "a.png");
    assert_eq!(e.dag.textures[0].address_u, AddressMode::Clamp);
    assert_eq!(e.dag.textures[0].address_v, AddressMode::Periodic);
    // The sample is followed by the conversion to the working space.
    assert!(matches!(e.dag.node(id).kind, NodeKind::Op { .. }), "{:?}", e.dag.node(id).kind);
    assert!(e.dag.nodes.iter().any(|n| matches!(n.kind, NodeKind::Sample { .. })));
}

#[test]
fn a_multioutput_node_is_asked_by_output() {
    let (s, o) = (Synthetic::new(), IrOptions::default());
    let mut e = standalone(&s, &o);
    let id = e
        .instantiate_output("separate3", "outy", &[("in", Arg::Value(Value::Vector3([1.0, 2.0, 3.0])))], "sep")
        .unwrap();
    assert_eq!(e.fold_constant(id), Some(Value::Float(2.0)));
}

#[test]
fn authored_values_follow_the_options() {
    let s = Synthetic::new();
    let uniforms = IrOptions::default();
    let mut e = standalone(&s, &uniforms);
    let id = e.authored("base_color", Value::Color3([1.0, 0.0, 0.0]), "p");
    assert!(matches!(&e.dag.node(id).kind, NodeKind::Uniform { name, .. } if name == "base_color"));
    let again = e.authored("base_color", Value::Color3([0.0, 1.0, 0.0]), "p");
    assert!(matches!(&e.dag.node(again).kind, NodeKind::Uniform { name, .. } if name == "base_color_2"));
    let constants = IrOptions { values_as_uniforms: false, ..IrOptions::default() };
    let mut e = standalone(&s, &constants);
    let id = e.authored("base_color", Value::Color3([1.0, 0.0, 0.0]), "p");
    assert!(matches!(e.dag.node(id).kind, NodeKind::Const(_)));
}

#[test]
fn an_unknown_category_is_an_error() {
    let (s, o) = (Synthetic::new(), IrOptions::default());
    let mut e = standalone(&s, &o);
    let err = e.instantiate("no_such_node", Type::Float, &[], "x").unwrap_err();
    assert!(matches!(err, Error::UnknownNode { .. }), "{err}");
}

#[test]
fn the_node_limit_applies() {
    let s = Synthetic::new();
    let o = IrOptions { max_nodes: Some(2), ..IrOptions::default() };
    let mut e = standalone(&s, &o);
    let err = e
        .instantiate(
            "overlay",
            Type::Color3,
            &[("fg", Arg::Value(Value::Color3([0.0; 3]))), ("bg", Arg::Value(Value::Color3([1.0; 3])))],
            "o",
        )
        .unwrap_err();
    assert!(matches!(err, Error::Limit(_)), "{err}");
}

#[test]
fn the_node_limit_applies_to_a_leaf() {
    let s = Synthetic::new();
    let o = IrOptions { max_nodes: Some(2), ..IrOptions::default() };
    let mut e = standalone(&s, &o);
    let err = e
        .instantiate(
            "add",
            Type::Float,
            &[("in1", Arg::Value(Value::Float(1.0))), ("in2", Arg::Value(Value::Float(2.0)))],
            "sum",
        )
        .unwrap_err();
    assert!(matches!(err, Error::Limit(_)), "{err}");
}

#[test]
fn instantiate_needs_a_standalone_elaborator() {
    let d = doc(r#"<nodegraph name="g"/>"#);
    let o = IrOptions::default();
    let mut e = Elaborator::new(&d, crate::library::standard(), &o, "");
    let err = e.instantiate("add", Type::Float, &[], "x").unwrap_err();
    assert!(matches!(&err, Error::BadValue { message, .. } if message.contains("standalone")), "{err}");
}
