//! `max_nodes`: the DAG stops growing at the limit, even when nodegraphs instanced inside one another double it
//! at every level.

use shadewire::Error;

use super::tests::doc;
use crate::library::standard;
use crate::{IrOptions, elaborate};

/// Level `k` of `levels` adds two instances of level `k - 1`, so the material needs about `2^levels` nodes.
fn doubling(levels: usize) -> String {
    let mut text = String::from(
        r#"<nodedef name="ND_l0_float" node="l0"><input name="in" type="float" value="0"/><output name="out" type="float"/></nodedef>
        <nodegraph name="NG_l0_float" nodedef="ND_l0_float">
          <add name="a" type="float"><input name="in1" type="float" interfacename="in"/><input name="in2" type="float" value="1"/></add>
          <output name="out" type="float" nodename="a"/>
        </nodegraph>"#,
    );
    for k in 1..=levels {
        let p = k - 1;
        text.push_str(&format!(
            r#"<nodedef name="ND_l{k}_float" node="l{k}"><input name="in" type="float" value="0"/><output name="out" type="float"/></nodedef>
            <nodegraph name="NG_l{k}_float" nodedef="ND_l{k}_float">
              <l{p} name="x" type="float"><input name="in" type="float" interfacename="in"/></l{p}>
              <l{p} name="y" type="float"><input name="in" type="float" nodename="x"/></l{p}>
              <add name="a" type="float"><input name="in1" type="float" nodename="x"/><input name="in2" type="float" nodename="y"/></add>
              <output name="out" type="float" nodename="a"/>
            </nodegraph>"#
        ));
    }
    text.push_str(&format!(
        r#"<l{levels} name="top" type="float"/>
        <standard_surface name="s" type="surfaceshader"><input name="base" type="float" nodename="top"/></standard_surface>
        <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>"#
    ));
    text
}

fn compile(levels: usize, max_nodes: Option<usize>) -> Result<shadewire::Dag, Error> {
    let options = IrOptions { max_nodes, ..IrOptions::with_models(&["standard_surface"]) };
    elaborate(&doc(&doubling(levels)), standard(), &options, None)
}

#[test]
fn a_graph_under_the_limit_compiles() {
    let unlimited = compile(4, None).unwrap();
    assert!(unlimited.nodes.len() > 1 << 4, "{} nodes do not double per level", unlimited.nodes.len());
    let limited = compile(4, Some(unlimited.nodes.len())).unwrap();
    assert_eq!(limited.nodes.len(), unlimited.nodes.len());
}

#[test]
fn a_graph_over_the_limit_is_an_error() {
    let needed = compile(4, None).unwrap().nodes.len();
    match compile(4, Some(needed - 1)) {
        Err(Error::Limit(message)) => {
            assert_eq!(message, format!("the material needs more than {} nodes (max_nodes)", needed - 1))
        }
        other => panic!("expected a limit error, got {other:?}"),
    }
}

/// About 2^40 nodes unlimited: the limit has to stop elaboration early.
#[test]
fn an_exponential_graph_stops_at_the_limit() {
    let started = std::time::Instant::now();
    assert!(matches!(compile(40, Some(4096)), Err(Error::Limit(_))));
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}
