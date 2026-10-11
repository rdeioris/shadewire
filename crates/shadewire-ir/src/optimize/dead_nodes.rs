use crate::dag::node_args;
use crate::{Dag, Node, NodeId, NodeKind, SurfaceField};

/// Drops nodes no surface field reaches and compacts the ids.
pub fn run(dag: &Dag) -> Dag {
    let mut live = vec![false; dag.nodes.len()];
    for field in &dag.surface.fields {
        live[field.node.index()] = true;
    }
    for index in (0..dag.nodes.len()).rev() {
        if live[index] {
            for arg in node_args(&dag.nodes[index].kind) {
                live[arg.index()] = true;
            }
        }
    }
    let mut new = Dag::new(&dag.surface.model);
    new.textures = dag.textures.clone();
    new.approximations = dag.approximations.clone();
    new.geomprop_defaults = dag.geomprop_defaults.clone();
    let mut map: Vec<Option<NodeId>> = vec![None; dag.nodes.len()];
    for (index, old) in dag.nodes.iter().enumerate() {
        if !live[index] {
            continue;
        }
        let kind = remap_kind(&old.kind, &map);
        map[index] = Some(new.add(Node { kind, ty: old.ty, origin: old.origin.clone() }));
    }
    new.surface.fields = dag
        .surface
        .fields
        .iter()
        .map(|f| SurfaceField { name: f.name.clone(), ty: f.ty, node: map[f.node.index()].expect("surface node is live") })
        .collect();
    new
}

fn remap_kind(kind: &NodeKind, map: &[Option<NodeId>]) -> NodeKind {
    let get = |id: &NodeId| map[id.index()].expect("argument of a live node is live");
    match kind {
        NodeKind::Op { op, args } => NodeKind::Op { op: *op, args: args.iter().map(get).collect() },
        NodeKind::Sample { texture, uv, default } => NodeKind::Sample { texture: *texture, uv: get(uv), default: default.clone() },
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn unused_nodes_disappear_and_ids_are_compacted() {
        let mut dag = Dag::new("m");
        dag.add(Node { kind: NodeKind::Const(Value::Float(9.0)), ty: Type::Float, origin: String::new() });
        let u = dag.add(Node { kind: NodeKind::Uniform { name: "k".into(), value: Value::Float(1.0) }, ty: Type::Float, origin: String::new() });
        let s = dag.add(Node { kind: NodeKind::Op { op: Op::Sin, args: vec![u] }, ty: Type::Float, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "f".into(), ty: Type::Float, node: s });
        let out = super::run(&dag);
        assert_eq!(out.nodes.len(), 2);
        assert_eq!(out.surface.fields[0].node, NodeId(1));
        assert_eq!(out.node(NodeId(1)).kind, NodeKind::Op { op: Op::Sin, args: vec![NodeId(0)] });
    }
}
