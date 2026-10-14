use shadewire::NodeKind;

use super::elaborate_output;
use super::tests::doc;

#[test]
fn an_inlined_library_node_is_a_compound() {
    let d = doc(r#"
        <nodegraph name="g">
          <overlay name="o" type="color3">
            <input name="fg" type="color3" value="0.2, 0.4, 0.6"/>
            <input name="bg" type="color3" value="0.5, 0.5, 0.5"/>
            <input name="mix" type="float" value="0.5"/>
          </overlay>
          <output name="out" type="color3" nodename="o"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    let c = dag.compounds.iter().find(|c| c.origin == "g/o").expect("the overlay instance is recorded");
    assert_eq!(c.nodedef, "ND_overlay_color3");
    let names: Vec<&str> = c.inputs.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["fg", "bg", "mix"]);
    assert_eq!(c.outputs, vec![("out".to_string(), dag.surface.fields[0].node)]);
    assert!(c.inputs.iter().all(|(_, id)| matches!(dag.node(*id).kind, NodeKind::Const(_))));
    // Library nodes inlined inside the overlay's own graph are compounds too, nested under it.
    assert!(dag.compounds.iter().all(|c| c.origin == "g/o" || c.origin.starts_with("g/o/")), "{:?}", dag.compounds);
    assert!(dag.compounds.len() > 1);
}

#[test]
fn a_leaf_node_is_not_a_compound() {
    let d = doc(r#"
        <nodegraph name="g">
          <multiply name="m" type="float">
            <input name="in1" type="float" value="2"/>
            <input name="in2" type="float" value="3"/>
          </multiply>
          <output name="out" type="float" nodename="m"/>
        </nodegraph>"#);
    assert!(elaborate_output(&d, "g", "out").unwrap().compounds.is_empty());
}

#[test]
fn nested_instances_are_nested_compounds() {
    let d = doc(r#"
        <nodedef name="ND_twice_overlay" node="twice_overlay">
          <input name="a" type="color3" value="0, 0, 0"/>
          <output name="out" type="color3"/>
        </nodedef>
        <nodegraph name="NG_twice_overlay" nodedef="ND_twice_overlay">
          <overlay name="inner" type="color3">
            <input name="fg" type="color3" interfacename="a"/>
            <input name="bg" type="color3" interfacename="a"/>
          </overlay>
          <output name="out" type="color3" nodename="inner"/>
        </nodegraph>
        <nodegraph name="g">
          <twice_overlay name="t" type="color3">
            <input name="a" type="color3" value="0.5, 0.5, 0.5"/>
          </twice_overlay>
          <output name="out" type="color3" nodename="t"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    let position = |origin: &str| {
        dag.compounds
            .iter()
            .position(|c| c.origin == origin)
            .unwrap_or_else(|| panic!("{origin} missing from {:?}", dag.compounds))
    };
    let inner = position("g/t/inner");
    let outer = position("g/t");
    // An instance is recorded when it returns, so the inner one comes first and the outermost last.
    assert!(inner < outer);
    assert_eq!(outer, dag.compounds.len() - 1);
    assert_eq!(dag.compounds[outer].nodedef, "ND_twice_overlay");
    assert_eq!(dag.compounds[inner].nodedef, "ND_overlay_color3");
    // Everything recorded sits under the outer instance.
    assert!(dag.compounds.iter().all(|c| c.origin == "g/t" || c.origin.starts_with("g/t/")), "{:?}", dag.compounds);
}
