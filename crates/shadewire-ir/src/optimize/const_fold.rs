use crate::interpret::ContextSource;
use crate::ops::broadcast::broadcast;
use crate::optimize::rebuild::Rebuilder;
use crate::{eval_op, ContextRead, Dag, Node, NodeKind, Space, TransformKind, Value};

/// Never called: only ops that do not read context are folded.
struct NoContext;

impl ContextSource for NoContext {
    fn read(&mut self, _read: &ContextRead) -> Value {
        unreachable!("const folding never reads context")
    }
    fn transform(&mut self, _kind: TransformKind, _from: Space, _to: Space, _v: [f32; 3]) -> [f32; 3] {
        unreachable!("const folding never transforms")
    }
}

/// Replaces ops whose arguments are all constants by their value.
pub fn run(dag: &Dag) -> Dag {
    let mut rebuilder = Rebuilder::new(dag);
    for old in &dag.nodes {
        match folded_value(&rebuilder, old) {
            Some(value) => {
                let id = rebuilder.new.add(Node { kind: NodeKind::Const(value), ty: old.ty, origin: old.origin.clone() });
                rebuilder.map.push(id);
            }
            None => {
                rebuilder.copy(old);
            }
        }
    }
    rebuilder.finish(dag)
}

fn folded_value(rebuilder: &Rebuilder, old: &Node) -> Option<Value> {
    let NodeKind::Op { op, args } = &old.kind else { return None };
    if op.reads_context() {
        return None;
    }
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        match &rebuilder.new.node(rebuilder.remap(*arg)).kind {
            NodeKind::Const(v) => values.push(v.clone()),
            _ => return None,
        }
    }
    Some(broadcast(&eval_op(op, &values, old.ty, &mut NoContext), old.ty))
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn folds_constant_subtrees_but_not_uniforms() {
        let mut dag = Dag::new("m");
        let a = dag.add(Node { kind: NodeKind::Const(Value::Float(2.0)), ty: Type::Float, origin: String::new() });
        let b = dag.add(Node { kind: NodeKind::Const(Value::Float(3.0)), ty: Type::Float, origin: String::new() });
        let s = dag.add(Node { kind: NodeKind::Op { op: Op::Multiply, args: vec![a, b] }, ty: Type::Float, origin: String::new() });
        let u = dag.add(Node { kind: NodeKind::Uniform { name: "k".into(), value: Value::Float(1.0) }, ty: Type::Float, origin: String::new() });
        let t = dag.add(Node { kind: NodeKind::Op { op: Op::Add, args: vec![s, u] }, ty: Type::Float, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "x".into(), ty: Type::Float, node: t });
        let out = super::run(&dag);
        let field = out.node(out.surface.fields[0].node);
        let NodeKind::Op { args, .. } = &field.kind else { panic!() };
        assert_eq!(out.node(args[0]).kind, NodeKind::Const(Value::Float(6.0)));
    }
}
