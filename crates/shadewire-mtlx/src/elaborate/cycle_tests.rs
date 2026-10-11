//! Cycles, memo sharing and bad interfaces.

use shadewire_ir::{Error, NodeKind};

use super::elaborate_output;
use super::tests::doc;
use crate::document::Document;

fn cycle_node(d: &Document, graph: &str) -> String {
    match elaborate_output(d, graph, "out") {
        Err(Error::Cycle { node, .. }) => node,
        other => panic!("expected a cycle, got {other:?}"),
    }
}

#[test]
fn self_implementing_nodedef_is_a_cycle() {
    let d = doc(
        r#"<nodedef name="ND_loop_float" node="loop"><output name="out" type="float"/></nodedef>
        <nodegraph name="NG_loop_float" nodedef="ND_loop_float">
          <loop name="again" type="float"/>
          <output name="out" type="float" nodename="again"/>
        </nodegraph>
        <nodegraph name="g">
          <loop name="l" type="float"/>
          <output name="out" type="float" nodename="l"/>
        </nodegraph>"#,
    );
    assert_eq!(cycle_node(&d, "g"), "again");
}

#[test]
fn mutually_implementing_nodedefs_are_a_cycle() {
    let d = doc(
        r#"<nodedef name="ND_ping_float" node="ping"><output name="out" type="float"/></nodedef>
        <nodedef name="ND_pong_float" node="pong"><output name="out" type="float"/></nodedef>
        <nodegraph name="NG_ping_float" nodedef="ND_ping_float">
          <pong name="p" type="float"/>
          <output name="out" type="float" nodename="p"/>
        </nodegraph>
        <nodegraph name="NG_pong_float" nodedef="ND_pong_float">
          <ping name="q" type="float"/>
          <output name="out" type="float" nodename="q"/>
        </nodegraph>
        <nodegraph name="g">
          <ping name="start" type="float"/>
          <output name="out" type="float" nodename="start"/>
        </nodegraph>"#,
    );
    assert_eq!(cycle_node(&d, "g"), "q");
}

#[test]
fn graph_output_pointing_at_itself_is_a_cycle() {
    let d = doc(
        r#"<nodegraph name="g">
          <output name="out" type="float" nodegraph="g" output="out"/>
        </nodegraph>"#,
    );
    assert_eq!(cycle_node(&d, "g"), "g/out");
}

#[test]
fn a_node_read_with_and_without_its_output_name_is_made_once() {
    let d = doc(
        r#"<nodegraph name="g">
          <sin name="s" type="float"><input name="in" type="float" value="1"/></sin>
          <add name="a" type="float">
            <input name="in1" type="float" nodename="s"/>
            <input name="in2" type="float" nodename="s" output="out"/>
          </add>
          <output name="out" type="float" nodename="a"/>
        </nodegraph>"#,
    );
    let dag = elaborate_output(&d, "g", "out").unwrap();
    let NodeKind::Op { args, .. } = &dag.node(dag.surface.fields[0].node).kind else { panic!("not an op") };
    assert_eq!(args[0], args[1]);
}

#[test]
fn a_failed_node_does_not_stay_in_progress() {
    let d = doc(
        r#"<nodegraph name="g">
          <add name="bad" type="float"><input name="in1" type="float" value="x"/></add>
          <output name="out" type="float" nodename="bad"/>
        </nodegraph>"#,
    );
    let options = super::ElaborateOptions::default();
    let mut e = super::Elaborator::new(&d, crate::library::standard(), &options, "test");
    let at = e.at(0, "g".to_string());
    let scope = e.user_graph_scope("g", &at).unwrap();
    assert!(matches!(e.node_output(&scope, "bad", None), Err(Error::BadValue { .. })));
    assert!(e.in_progress.is_empty());
    assert!(matches!(e.node_output(&scope, "bad", None), Err(Error::BadValue { .. })));
}

fn interface_error(port: &str) -> String {
    let d = doc(&format!(
        r#"<nodegraph name="g">
          {port}
          <constant name="c" type="float"><input name="value" type="float" value="1"/></constant>
          <output name="out" type="float" nodename="c"/>
        </nodegraph>"#
    ));
    match elaborate_output(&d, "g", "out") {
        Err(Error::BadValue { at, .. }) => at.path,
        other => panic!("expected a bad value, got {other:?}"),
    }
}

#[test]
fn interface_inputs_with_nodename_or_odd_types_are_errors() {
    assert_eq!(interface_error(r#"<input name="k" type="float" nodename="c"/>"#), "g/k");
    assert_eq!(interface_error(r#"<input name="s" type="surfaceshader"/>"#), "g/s");
}
