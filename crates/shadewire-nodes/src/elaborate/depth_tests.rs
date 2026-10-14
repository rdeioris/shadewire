//! Deep graphs: a chain past the depth limit is an error, never a stack overflow.

use shadewire::Error;

use super::tests::doc;
use crate::library::standard;
use crate::{IrOptions, elaborate};

/// A material whose standard_surface `base` is the end of a chain of `n` adds.
fn add_chain(n: usize) -> String {
    let mut text =
        String::from(r#"<constant name="a0" type="float"><input name="value" type="float" value="1"/></constant>"#);
    for i in 1..=n {
        let previous = i - 1;
        text.push_str(&format!(
            r#"<add name="a{i}" type="float"><input name="in1" type="float" nodename="a{previous}"/><input name="in2" type="float" value="1"/></add>"#
        ));
    }
    text.push_str(&format!(
        r#"<standard_surface name="s" type="surfaceshader"><input name="base" type="float" nodename="a{n}"/></standard_surface>
        <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>"#
    ));
    text
}

fn compile_chain(n: usize) -> Result<shadewire::Dag, Error> {
    let d = doc(&add_chain(n));
    elaborate(&d, standard(), &IrOptions::with_models(&["standard_surface"]), None)
}

#[test]
fn a_chain_of_two_hundred_compiles() {
    let dag = compile_chain(200).unwrap();
    assert!(dag.surface.fields.iter().any(|f| f.name == "base"));
}

#[test]
fn a_chain_past_the_limit_is_an_error() {
    match compile_chain(1100) {
        Err(Error::BadValue { message, .. }) => assert_eq!(message, "graph deeper than 1024 nodes"),
        other => panic!("expected a bad value, got {other:?}"),
    }
}
