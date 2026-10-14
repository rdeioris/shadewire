use shadewire::interpret::{NoTextures, eval_surface};
use shadewire::{ClosureNode, Error, NodeKind, Type, Value};

use super::tests::{Zeros, inline};
use crate::document::PortType;
use crate::elaborate::{Arg, Elaborator, elaborate};
use crate::library::standard;
use crate::options::IrOptions;
use crate::surface::finish::ShaderNode;
use crate::synthetic::Synthetic;

const RED: Value = Value::Color3([1.0, 0.0, 0.0]);

#[test]
fn an_accepted_model_keeps_its_fields() {
    let (s, o) = (Synthetic::new(), IrOptions::default());
    let e = Elaborator::standalone(&s, &o);
    let dag = e.finish_model("open_pbr_surface", &[("base_color", Arg::Value(RED))], "mat").unwrap();
    assert_eq!(dag.surface.model, "open_pbr_surface");
    assert!(dag.surface.closure.is_none());
    let base = dag.surface.fields.iter().find(|f| f.name == "base_color").unwrap();
    assert_eq!(dag.node(base.node).kind, NodeKind::Const(RED));
    let weight = dag.surface.fields.iter().find(|f| f.name == "base_weight").unwrap();
    assert_eq!(weight.ty, Type::Float);
    // Fields not given take the nodedef's default.
    assert!(matches!(dag.node(weight.node).kind, NodeKind::Const(Value::Float(w)) if w == 1.0));
}

#[test]
fn a_model_the_host_does_not_list_opens_into_lobes_as_a_document_does() {
    let s = Synthetic::new();
    let o = IrOptions::with_models(&["surface"]);
    let e = Elaborator::standalone(&s, &o);
    let from_args = e.finish_model("open_pbr_surface", &[("base_color", Arg::Value(RED))], "mat").unwrap();
    let doc = inline(
        r#"
        <open_pbr_surface name="mat" type="surfaceshader">
          <input name="base_color" type="color3" value="1, 0, 0"/>
        </open_pbr_surface>
        <surfacematerial name="m" type="material">
          <input name="surfaceshader" type="surfaceshader" nodename="mat"/>
        </surfacematerial>"#,
    );
    let from_doc = elaborate(&doc, standard(), &o, None).unwrap();
    assert_eq!(from_args.surface.model, "surface");
    assert_eq!(from_args.surface.closure, from_doc.surface.closure);
    let a = eval_surface(&from_args, &mut Zeros, &NoTextures);
    let b = eval_surface(&from_doc, &mut Zeros, &NoTextures);
    assert_eq!(a, b);
}

#[test]
fn a_model_outside_the_options_is_no_translation() {
    let s = Synthetic::new();
    let o = IrOptions::with_models(&["standard_surface"]);
    let e = Elaborator::standalone(&s, &o);
    let err = e.finish_model("open_pbr_surface", &[], "mat").unwrap_err();
    assert!(matches!(err, Error::NoTranslation { .. }), "{err}");
}

fn lobe(name: &str, category: &str, closure: &str, inputs: Vec<(String, Arg)>) -> ShaderNode {
    ShaderNode {
        name: name.to_string(),
        category: category.to_string(),
        port_type: PortType::Closure(closure.to_string()),
        inputs,
        links: Vec::new(),
    }
}

#[test]
fn a_closure_tree_from_shader_nodes() {
    let s = Synthetic::new();
    let o = IrOptions::with_models(&["surface"]);
    let mut e = Elaborator::standalone(&s, &o);
    let roughness = e.authored("roughness", Value::Float(0.5), "d.roughness");
    let nodes = vec![
        lobe(
            "d",
            "oren_nayar_diffuse_bsdf",
            "BSDF",
            vec![("color".into(), Arg::Value(RED)), ("roughness".into(), Arg::Node(roughness))],
        ),
        lobe("e", "uniform_edf", "EDF", vec![("color".into(), Arg::Value(Value::Color3([0.0, 0.0, 2.0])))]),
        ShaderNode {
            name: "surface".into(),
            category: "surface".into(),
            port_type: PortType::SurfaceShader,
            inputs: vec![("opacity".into(), Arg::Value(Value::Float(0.75)))],
            links: vec![("bsdf".into(), "d".into()), ("edf".into(), "e".into())],
        },
    ];
    let dag = e.finish_closure(nodes, "surface").unwrap();
    assert_eq!(dag.surface.model, "surface");
    let closure = dag.surface.closure.as_ref().unwrap();
    assert!(closure.bsdf.is_some() && closure.edf.is_some());
    let kinds: Vec<&str> = closure
        .nodes
        .iter()
        .filter_map(|n| match n {
            ClosureNode::Lobe { kind, .. } => Some(kind.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, ["oren_nayar_diffuse_bsdf", "uniform_edf"]);
    let opacity = closure.opacity.as_ref().unwrap();
    let field = dag.surface.fields.iter().find(|f| &f.name == opacity).unwrap();
    assert_eq!(dag.node(field.node).kind, NodeKind::Const(Value::Float(0.75)));
    assert!(dag.nodes.iter().any(|n| matches!(&n.kind, NodeKind::Uniform { name, .. } if name == "roughness")));
}

#[test]
fn a_closure_tree_needs_the_surface_model_accepted() {
    let (s, o) = (Synthetic::new(), IrOptions::default());
    let e = Elaborator::standalone(&s, &o);
    let nodes = vec![ShaderNode {
        name: "surface".into(),
        category: "surface".into(),
        port_type: PortType::SurfaceShader,
        inputs: Vec::new(),
        links: Vec::new(),
    }];
    let err = e.finish_closure(nodes, "surface").unwrap_err();
    assert!(matches!(err, Error::NoTranslation { .. }), "{err}");
}
