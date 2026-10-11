pub mod const_fold;
pub mod cse;
pub mod dead_nodes;
pub mod identities;
pub mod rebuild;

use crate::Dag;

/// Folds constants, removes identities, shares equal nodes, drops unused nodes.
pub fn optimize(dag: &Dag) -> Dag {
    let folded = const_fold::run(dag);
    let simplified = identities::run(&folded);
    let shared = cse::run(&simplified);
    dead_nodes::run(&shared)
}

#[cfg(test)]
mod tests {
    use crate::*;

    fn n(kind: NodeKind) -> Node {
        Node { kind, ty: Type::Float, origin: String::new() }
    }

    #[test]
    fn whole_pipeline_folds_simplifies_shares_and_compacts() {
        let mut dag = Dag::new("m");
        let one = dag.add(n(NodeKind::Const(Value::Float(1.0))));
        let two = dag.add(n(NodeKind::Const(Value::Float(2.0))));
        let three = dag.add(n(NodeKind::Op { op: Op::Add, args: vec![one, two] }));
        let u = dag.add(n(NodeKind::Uniform { name: "k".into(), value: Value::Float(0.5) }));
        let s1 = dag.add(n(NodeKind::Op { op: Op::Sin, args: vec![u] }));
        let s2 = dag.add(n(NodeKind::Op { op: Op::Sin, args: vec![u] }));
        let m = dag.add(n(NodeKind::Op { op: Op::Multiply, args: vec![s1, one] }));
        let sum = dag.add(n(NodeKind::Op { op: Op::Add, args: vec![m, s2] }));
        let total = dag.add(n(NodeKind::Op { op: Op::Add, args: vec![sum, three] }));
        dag.surface.fields.push(SurfaceField { name: "x".into(), ty: Type::Float, node: total });
        let out = super::optimize(&dag);
        // live nodes: uniform, sin, sum(sin, sin), const 3.0, total
        let NodeKind::Op { args, .. } = &out.node(out.surface.fields[0].node).kind else { panic!() };
        assert_eq!(out.node(args[1]).kind, NodeKind::Const(Value::Float(3.0)));
        let NodeKind::Op { args: inner, .. } = &out.node(args[0]).kind else { panic!() };
        assert_eq!(inner[0], inner[1]);
        assert_eq!(out.nodes.len(), 5);
    }
}
