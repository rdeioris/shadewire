use crate::{Dag, NodeKind, Value};

/// How the host provides a node's value.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
    Constant(Value),
    Uniform,
    Varying,
}

pub fn node_kinds(dag: &Dag) -> Vec<ParamKind> {
    let mut kinds: Vec<ParamKind> = Vec::with_capacity(dag.nodes.len());
    for node in &dag.nodes {
        let kind = match &node.kind {
            NodeKind::Const(v) => ParamKind::Constant(v.clone()),
            NodeKind::Uniform { .. } => ParamKind::Uniform,
            NodeKind::Context(_) | NodeKind::Sample { .. } => ParamKind::Varying,
            NodeKind::Op { op, args } => {
                if op.reads_context() || args.iter().any(|a| kinds[a.index()] == ParamKind::Varying) {
                    ParamKind::Varying
                } else {
                    // An unfolded constant op counts as uniform; it only appears without optimize.
                    ParamKind::Uniform
                }
            }
        };
        kinds.push(kind);
    }
    kinds
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn varying_wins_over_uniform_wins_over_constant() {
        let mut dag = Dag::new("m");
        let c = dag.add(Node { kind: NodeKind::Const(Value::Float(1.0)), ty: Type::Float, origin: String::new() });
        let u = dag.add(Node {
            kind: NodeKind::Uniform { name: "k".into(), value: Value::Float(1.0) },
            ty: Type::Float,
            origin: String::new(),
        });
        let t = dag.add(Node { kind: NodeKind::Context(ContextRead::Time), ty: Type::Float, origin: String::new() });
        let cu = dag.add(Node {
            kind: NodeKind::Op { op: Op::Add, args: vec![c, u] },
            ty: Type::Float,
            origin: String::new(),
        });
        let ut = dag.add(Node {
            kind: NodeKind::Op { op: Op::Add, args: vec![u, t] },
            ty: Type::Float,
            origin: String::new(),
        });
        let kinds = classify::node_kinds(&dag);
        assert_eq!(kinds[c.index()], ParamKind::Constant(Value::Float(1.0)));
        assert_eq!(kinds[cu.index()], ParamKind::Uniform);
        assert_eq!(kinds[ut.index()], ParamKind::Varying);
    }
}
