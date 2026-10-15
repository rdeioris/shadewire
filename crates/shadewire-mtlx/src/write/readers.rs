//! How many readers every written node has, and which constants may be written as the value of their only
//! reader.

use std::collections::HashMap;

use shadewire::{Dag, NodeId, NodeKind};
use shadewire_nodes::Index;
use shadewire_nodes::document::NodeDef;

use super::catalog::{arg_names, category_for, nodedef_for};
use super::plan::Plan;
use super::shader::shader_reads;

/// Counts every written reference, and marks the consts whose only reader may take them as a value.
pub(crate) fn count_readers(dag: &Dag, index: &Index, plan: &mut Plan) {
    // (node read, whether that read is an input that may carry the node's value instead)
    let mut reads: Vec<(NodeId, bool)> = Vec::new();
    for (position, node) in dag.nodes.iter().enumerate() {
        if plan.interior.contains(&NodeId(position as u32)) {
            continue;
        }
        match &node.kind {
            NodeKind::Op { op, args } => {
                let arg_types: Vec<_> = args.iter().map(|a| dag.node(*a).ty).collect();
                let nd = nodedef_for(index, &category_for(op, args.len()).name, node.ty, &arg_types);
                let names: Vec<&str> = nd.map(|nd| arg_names(op, nd)).unwrap_or_default();
                for (i, arg) in args.iter().enumerate() {
                    let geomprop = nd.zip(names.get(i)).is_some_and(|(nd, name)| reads_geomprop(nd, name));
                    reads.push((*arg, !geomprop));
                }
            }
            NodeKind::Sample { uv, grad, .. } => {
                reads.push((*uv, false));
                reads.extend(grad.iter().flatten().map(|g| (*g, false)));
            }
            NodeKind::Const(_) | NodeKind::Uniform { .. } | NodeKind::Context(_) => {}
        }
    }
    for position in &plan.kept {
        let compound = &dag.compounds[*position];
        let nd = index.nodedef(&compound.nodedef);
        for (name, id) in &compound.inputs {
            reads.push((*id, !nd.is_some_and(|nd| reads_geomprop(nd, name))));
        }
    }
    reads.extend(shader_reads(dag, index).into_iter().map(|id| (id, false)));
    let mut single: HashMap<NodeId, bool> = HashMap::new();
    for (id, as_value) in reads {
        *plan.readers.entry(id).or_insert(0) += 1;
        single.insert(id, as_value);
    }
    for (id, as_value) in single {
        if as_value && plan.readers[&id] == 1 && matches!(dag.node(id).kind, NodeKind::Const(_)) {
            plan.inline_const.insert(id);
        }
    }
}

/// An input with a `defaultgeomprop` ignores an authored value on read: a const there must stay a node.
pub(crate) fn reads_geomprop(nd: &NodeDef, input: &str) -> bool {
    nd.inputs.iter().any(|p| p.name == input && p.default_geomprop.is_some())
}
