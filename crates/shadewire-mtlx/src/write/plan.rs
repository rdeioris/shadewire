//! Which dag nodes a document writes, and how: library nodes kept as one node, colorspace conversions folded
//! into their image, constants written as values, and how many readers every node has.

use std::collections::{HashMap, HashSet};

use shadewire::{Compound, Dag, NodeId, NodeKind};
use shadewire_nodes::Index;
use shadewire_nodes::document::{NodeDef, PortType};

use super::readers::count_readers;

/// The emission plan of a dag.
#[derive(Debug, Default)]
pub struct Plan {
    /// Compounds written as one node, outermost only; by position in `dag.compounds`.
    pub kept: Vec<usize>,
    /// Nodes inside a kept compound (origin under its origin): never written themselves.
    pub interior: HashSet<NodeId>,
    /// A Const read by exactly one pattern input and nothing else: written as that input's value.
    pub inline_const: HashSet<NodeId>,
    /// Colorspace conversions folded into their image: the compound index, and the Sample node whose `image`
    /// element stands for the compound's output.
    pub folded_conversions: HashMap<usize, NodeId>,
    /// How many readers each node has (op args, sample uv/grad, compound inputs, fields, closure reads).
    pub readers: HashMap<NodeId, usize>,
    /// `compound {nodedef} at {origin} written expanded: not a library nodedef`, one per such compound.
    pub(crate) warnings: Vec<String>,
    /// Interior nodes read as a collapsed compound's output: an instance followed through several outputs is
    /// inlined once per output, and `Compound` records the first copy, so a later copy's node with the origin of
    /// a recorded output computes that output again.
    pub(crate) aliases: HashMap<NodeId, NodeId>,
}

/// Whether `origin` is `compound` itself or inside it.
pub(crate) fn is_under(origin: &str, compound: &str) -> bool {
    origin == compound || origin.strip_prefix(compound).is_some_and(|rest| rest.starts_with('/'))
}

/// The plan for `dag`: with `inline`, no compound is kept (colorspace conversions are still folded, since the
/// image's `colorspace` attribute already converts on read).
pub fn plan(dag: &Dag, index: &Index, inline: bool) -> Plan {
    let mut plan = Plan::default();
    let mut order: Vec<usize> = (0..dag.compounds.len()).collect();
    order.sort_by(|a, b| dag.compounds[*a].origin.cmp(&dag.compounds[*b].origin));
    let mut taken: Vec<&str> = Vec::new();
    for position in order {
        let compound = &dag.compounds[position];
        if taken.iter().any(|origin| is_under(&compound.origin, origin)) || !outputs_inside(dag, compound) {
            continue;
        }
        if let Some(sample) = converted_sample(dag, compound) {
            plan.folded_conversions.insert(position, sample);
            taken.push(&compound.origin);
            continue;
        }
        if inline {
            continue;
        }
        match index.nodedef(&compound.nodedef) {
            None => plan.warnings.push(format!(
                "compound {} at {} written expanded: not a library nodedef",
                compound.nodedef, compound.origin
            )),
            Some(nd) if !has_text_input(nd) => {
                plan.kept.push(position);
                taken.push(&compound.origin);
            }
            Some(_) => {}
        }
    }
    plan.kept.sort_unstable();
    let collapsed: Vec<usize> = plan.kept.iter().chain(plan.folded_conversions.keys()).copied().collect();
    for position in collapsed {
        let compound = &dag.compounds[position];
        let outputs: HashMap<&str, NodeId> =
            compound.outputs.iter().map(|(_, id)| (dag.node(*id).origin.as_str(), *id)).collect();
        for id in interior(dag, compound) {
            // A recorded output is never an alias: two outputs of one inner multi-output node share an origin.
            if let Some(output) = outputs.get(dag.node(id).origin.as_str())
                && !compound.outputs.iter().any(|(_, recorded)| *recorded == id)
            {
                plan.aliases.insert(id, *output);
            }
            plan.interior.insert(id);
        }
    }
    count_readers(dag, index, &mut plan);
    plan
}

/// Whether every output the compound computes is a node inside it (a pass-through has nothing to collapse).
fn outputs_inside(dag: &Dag, compound: &Compound) -> bool {
    !compound.outputs.is_empty()
        && compound.outputs.iter().all(|(_, id)| {
            !compound.inputs.iter().any(|(_, input)| input == id) && is_under(&dag.node(*id).origin, &compound.origin)
        })
}

/// A string or filename input: `Compound` records value inputs only, so an instance's text is unknown.
fn has_text_input(nd: &NodeDef) -> bool {
    nd.inputs.iter().any(|p| matches!(p.port_type, PortType::String | PortType::Filename))
}

/// The Sample a `ND_{cs}_to_{working}_{type}` conversion reads, when that Sample's texture is in colorspace `cs`.
pub(crate) fn converted_sample(dag: &Dag, compound: &Compound) -> Option<NodeId> {
    let [(_, input)] = compound.inputs.as_slice() else { return None };
    let node = dag.node(*input);
    let NodeKind::Sample { texture, .. } = &node.kind else { return None };
    let cs = dag.textures[*texture].colorspace.as_deref()?;
    let converts = compound.nodedef.starts_with(&format!("ND_{cs}_to_"))
        && compound.nodedef.ends_with(&format!("_{}", node.ty.mtlx_name()));
    converts.then_some(*input)
}

/// The nodes of a collapsed compound: every node under its origin except its own inputs.
fn interior(dag: &Dag, compound: &Compound) -> Vec<NodeId> {
    let inputs: HashSet<NodeId> = compound.inputs.iter().map(|(_, id)| *id).collect();
    (0..dag.nodes.len() as u32)
        .map(NodeId)
        .filter(|id| !inputs.contains(id) && is_under(&dag.node(*id).origin, &compound.origin))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IrOptions, Resolver};
    use shadewire::{Op, SurfaceField, Type, Value};
    use shadewire_nodes::document::empty_document;
    use shadewire_nodes::library::standard;

    /// The dag of a document whose `surface_unlit` reads output `out` of nodegraph `g` into `input`.
    fn dag_of(graph: &str, input: &str, ty: &str) -> Dag {
        let text = format!(
            r#"<materialx version="1.39">{graph}
              <surface_unlit name="s" type="surfaceshader">
                <input name="{input}" type="{ty}" nodegraph="g" output="out"/>
              </surface_unlit>
              <surfacematerial name="m" type="material">
                <input name="surfaceshader" type="surfaceshader" nodename="s"/>
              </surfacematerial>
            </materialx>"#
        );
        crate::to_ir(&text, None, None, &IrOptions::default(), &Resolver::default()).unwrap().dag
    }

    fn id_of(dag: &Dag, origin: &str) -> NodeId {
        NodeId(dag.nodes.iter().position(|n| n.origin == origin).unwrap() as u32)
    }

    #[test]
    fn a_colorspace_conversion_is_folded_into_its_image_in_both_modes() {
        let dag = dag_of(
            r#"<nodegraph name="g">
              <image name="img" type="color3">
                <input name="file" type="filename" value="a.png" colorspace="srgb_texture"/>
              </image>
              <output name="out" type="color3" nodename="img"/>
            </nodegraph>"#,
            "emission_color",
            "color3",
        );
        let index = Index::new(empty_document(), standard());
        let sample = dag.nodes.iter().position(|n| matches!(n.kind, NodeKind::Sample { .. })).unwrap();
        for inline in [false, true] {
            let plan = plan(&dag, &index, inline);
            assert_eq!(plan.folded_conversions.values().collect::<Vec<_>>(), [&NodeId(sample as u32)], "{inline}");
            assert!(plan.kept.is_empty() && !plan.interior.is_empty(), "{inline}: {plan:?}");
        }
    }

    #[test]
    fn a_constant_on_an_input_with_a_defaultgeomprop_stays_a_node() {
        // A value authored there would be ignored on read: the input reads its geometric property instead.
        let dag = dag_of(
            r#"<nodegraph name="g">
              <constant name="c" type="vector2"><input name="value" type="vector2" value="0.5, 0.25"/></constant>
              <noise2d name="n" type="float"><input name="texcoord" type="vector2" nodename="c"/></noise2d>
              <output name="out" type="float" nodename="n"/>
            </nodegraph>"#,
            "emission",
            "float",
        );
        let plan = plan(&dag, &Index::new(empty_document(), standard()), false);
        assert!(!plan.inline_const.contains(&id_of(&dag, "g/c.value")), "{plan:?}");
        assert!(plan.inline_const.contains(&id_of(&dag, "g/n.amplitude")), "{plan:?}");
    }

    #[test]
    fn an_instance_inlined_twice_reads_its_second_copy_from_the_first() {
        // A multi-output node followed through two outputs is inlined twice; one Compound records both.
        let mut dag = Dag::new("surface_unlit");
        let node = |kind, origin: &str| shadewire::Node { kind, ty: Type::Color3, origin: origin.to_string() };
        let fg = dag.add(node(NodeKind::Const(Value::Color3([0.2; 3])), "g/o.fg"));
        let bg = dag.add(node(NodeKind::Const(Value::Color3([0.5; 3])), "g/o.bg"));
        let first = dag.add(node(NodeKind::Op { op: Op::Add, args: vec![fg, bg] }, "g/o/x"));
        let second = dag.add(node(NodeKind::Op { op: Op::Add, args: vec![fg, bg] }, "g/o/x"));
        let read = dag.add(node(NodeKind::Op { op: Op::Multiply, args: vec![second, first] }, "g/m"));
        dag.surface.fields.push(SurfaceField { name: "emission_color".into(), ty: Type::Color3, node: read });
        dag.compounds.push(Compound {
            nodedef: "ND_overlay_color3".into(),
            inputs: vec![("fg".into(), fg), ("bg".into(), bg)],
            outputs: vec![("out".into(), first)],
            origin: "g/o".into(),
        });
        let plan = plan(&dag, &Index::new(empty_document(), standard()), false);
        assert_eq!(plan.kept, [0]);
        assert_eq!(plan.aliases, HashMap::from([(second, first)]));
    }

    #[test]
    fn two_outputs_of_one_inner_node_are_never_aliased_to_each_other() {
        // Both outputs come from one inner multi-output node, so they share its origin.
        let mut dag = Dag::new("surface_unlit");
        let node = |kind, origin: &str| shadewire::Node { kind, ty: Type::Color3, origin: origin.to_string() };
        let r = dag.add(node(NodeKind::Const(Value::Color3([0.9, 0.5, 0.3])), "g/a.reflectivity"));
        let ior = dag.add(node(NodeKind::Op { op: Op::Add, args: vec![r, r] }, "g/a/x"));
        let extinction = dag.add(node(NodeKind::Op { op: Op::Multiply, args: vec![r, r] }, "g/a/x"));
        let read = dag.add(node(NodeKind::Op { op: Op::Add, args: vec![ior, extinction] }, "g/s"));
        dag.surface.fields.push(SurfaceField { name: "emission_color".into(), ty: Type::Color3, node: read });
        dag.compounds.push(Compound {
            nodedef: "ND_artistic_ior".into(),
            inputs: vec![("reflectivity".into(), r)],
            outputs: vec![("ior".into(), ior), ("extinction".into(), extinction)],
            origin: "g/a".into(),
        });
        let plan = plan(&dag, &Index::new(empty_document(), standard()), false);
        assert_eq!(plan.kept, [0]);
        assert!(plan.aliases.is_empty(), "{:?}", plan.aliases);
    }

    #[test]
    fn a_user_nodedef_is_expanded_with_a_warning_unless_inlining() {
        let dag = dag_of(
            r#"<nodedef name="ND_twice" node="twice">
              <input name="in" type="float" value="0"/><output name="out" type="float"/>
            </nodedef>
            <nodegraph name="NG_twice" nodedef="ND_twice">
              <multiply name="m" type="float">
                <input name="in1" type="float" interfacename="in"/><input name="in2" type="float" value="2"/>
              </multiply>
              <output name="out" type="float" nodename="m"/>
            </nodegraph>
            <nodegraph name="g">
              <twice name="t" type="float"><input name="in" type="float" value="0.25"/></twice>
              <output name="out" type="float" nodename="t"/>
            </nodegraph>"#,
            "emission",
            "float",
        );
        let index = Index::new(empty_document(), standard());
        let kept = plan(&dag, &index, false);
        assert_eq!(kept.warnings, ["compound ND_twice at g/t written expanded: not a library nodedef"]);
        assert!(kept.kept.is_empty() && kept.interior.is_empty(), "{kept:?}");
        assert!(plan(&dag, &index, true).warnings.is_empty());
    }
}
