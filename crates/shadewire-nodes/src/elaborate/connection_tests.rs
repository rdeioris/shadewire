//! Connections must carry the type the input declares (or a same-width relabel).

use shadewire::{Error, Type};

use super::elaborate_output;
use super::tests::doc;
use crate::library::standard;
use crate::{ElaborateOptions, elaborate};

fn mismatch(result: Result<shadewire::Dag, Error>) -> (String, String, String) {
    match result {
        Err(Error::TypeMismatch { at, expected, found }) => (at.path, expected, found),
        other => panic!("expected a type mismatch, got {other:?}"),
    }
}

fn add_fed_by(source: &str) -> (String, String, String) {
    let d = doc(&format!(
        r#"<nodegraph name="g">
          {source}
          <add name="a" type="float">
            <input name="in1" type="float" nodename="src"/>
            <input name="in2" type="float" value="1"/>
          </add>
          <output name="out" type="float" nodename="a"/>
        </nodegraph>"#
    ));
    mismatch(elaborate_output(&d, "g", "out"))
}

#[test]
fn a_vector2_node_into_a_float_input_is_a_mismatch() {
    let source = r#"<constant name="src" type="vector2"><input name="value" type="vector2" value="1, 2"/></constant>"#;
    let (path, expected, found) = add_fed_by(source);
    assert_eq!((path.as_str(), expected.as_str(), found.as_str()), ("g/a.in1", "float", "vector2"));
}

#[test]
fn a_texcoord_into_a_float_input_is_a_mismatch() {
    let (path, expected, found) = add_fed_by(r#"<texcoord name="src" type="vector2"/>"#);
    assert_eq!((path.as_str(), expected.as_str(), found.as_str()), ("g/a.in1", "float", "vector2"));
}

#[test]
fn a_texcoord_into_base_color_is_a_mismatch() {
    let d = doc(r#"<texcoord name="uv" type="vector2"/>
        <standard_surface name="s" type="surfaceshader">
          <input name="base_color" type="color3" nodename="uv"/>
        </standard_surface>
        <surfacematerial name="m" type="material">
          <input name="surfaceshader" type="surfaceshader" nodename="s"/>
        </surfacematerial>"#);
    let result = elaborate(&d, standard(), &ElaborateOptions::default(), None, &["standard_surface"]);
    let (path, expected, found) = mismatch(result);
    assert_eq!((path.as_str(), expected.as_str(), found.as_str()), ("s.base_color", "color3", "vector2"));
}

#[test]
fn a_vector3_node_into_a_color3_input_is_relabelled() {
    let d = doc(r#"<constant name="v" type="vector3"><input name="value" type="vector3" value="1, 2, 3"/></constant>
        <standard_surface name="s" type="surfaceshader">
          <input name="base_color" type="color3" nodename="v"/>
        </standard_surface>
        <surfacematerial name="m" type="material">
          <input name="surfaceshader" type="surfaceshader" nodename="s"/>
        </surfacematerial>"#);
    let dag = elaborate(&d, standard(), &ElaborateOptions::default(), None, &["standard_surface"]).unwrap();
    let field = dag.surface.fields.iter().find(|f| f.name == "base_color").unwrap();
    assert_eq!(field.ty, Type::Color3);
}
