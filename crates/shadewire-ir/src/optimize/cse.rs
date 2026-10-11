use std::collections::HashMap;

use crate::optimize::rebuild::Rebuilder;
use crate::{Dag, NodeId};

/// Merges nodes with the same kind, arguments and type.
pub fn run(dag: &Dag) -> Dag {
    let mut rebuilder = Rebuilder::new(dag);
    let mut seen: HashMap<String, NodeId> = HashMap::new();
    for old in &dag.nodes {
        // Debug of f32 round-trips, so equal text means equal value.
        let key = format!("{:?}|{:?}", rebuilder.remapped_kind(&old.kind), old.ty);
        match seen.get(&key) {
            Some(existing) => rebuilder.alias(*existing),
            None => {
                let id = rebuilder.copy(old);
                seen.insert(key, id);
            }
        }
    }
    rebuilder.finish(dag)
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn identical_sins_become_one() {
        let mut dag = Dag::new("m");
        let u = dag.add(Node {
            kind: NodeKind::Uniform { name: "k".into(), value: Value::Float(1.0) },
            ty: Type::Float,
            origin: String::new(),
        });
        let a =
            dag.add(Node { kind: NodeKind::Op { op: Op::Sin, args: vec![u] }, ty: Type::Float, origin: String::new() });
        let b =
            dag.add(Node { kind: NodeKind::Op { op: Op::Sin, args: vec![u] }, ty: Type::Float, origin: String::new() });
        let s = dag.add(Node {
            kind: NodeKind::Op { op: Op::Add, args: vec![a, b] },
            ty: Type::Float,
            origin: String::new(),
        });
        dag.surface.fields.push(SurfaceField { name: "f".into(), ty: Type::Float, node: s });
        let out = super::run(&dag);
        assert_eq!(out.nodes.len(), 3);
        let NodeKind::Op { args, .. } = &out.node(out.surface.fields[0].node).kind else { panic!() };
        assert_eq!(args[0], args[1]);
    }
}
