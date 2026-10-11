use crate::optimize::rebuild::Rebuilder;
use crate::{Dag, Node, NodeId, NodeKind, Op};

/// Removes `x * 1`, `x + 0`, `x - 0`, `x / 1` and `mix` with a constant 0 or 1 factor.
pub fn run(dag: &Dag) -> Dag {
    let mut rebuilder = Rebuilder::new(dag);
    for old in &dag.nodes {
        match replacement(&rebuilder, old) {
            Some(existing) => rebuilder.alias(existing),
            None => {
                rebuilder.copy(old);
            }
        }
    }
    rebuilder.finish(dag)
}

/// The new node that already holds this node's value, if there is one.
fn replacement(rebuilder: &Rebuilder, old: &Node) -> Option<NodeId> {
    let NodeKind::Op { op, args } = &old.kind else { return None };
    let new_args: Vec<NodeId> = args.iter().map(|a| rebuilder.remap(*a)).collect();
    let same_type = |id: NodeId| rebuilder.new.node(id).ty == old.ty;
    match op {
        Op::Multiply => either_side(rebuilder, &new_args, 1.0).filter(|id| same_type(*id)),
        Op::Add => either_side(rebuilder, &new_args, 0.0).filter(|id| same_type(*id)),
        Op::Subtract => right_side(rebuilder, &new_args, 0.0).filter(|id| same_type(*id)),
        Op::Divide => right_side(rebuilder, &new_args, 1.0).filter(|id| same_type(*id)),
        Op::Mix => {
            let factor = constant_floats(rebuilder, new_args[2])?;
            let (fg, bg) = (new_args[0], new_args[1]);
            if factor.iter().all(|x| *x == 0.0) && same_type(bg) {
                Some(bg)
            } else if factor.iter().all(|x| *x == 1.0) && same_type(fg) {
                Some(fg)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// For a two-argument op: if one side is the constant `identity`, the other side.
fn either_side(rebuilder: &Rebuilder, args: &[NodeId], identity: f32) -> Option<NodeId> {
    let is_identity = |id: NodeId| constant_floats(rebuilder, id).is_some_and(|f| f.iter().all(|x| *x == identity));
    if is_identity(args[1]) {
        Some(args[0])
    } else if is_identity(args[0]) {
        Some(args[1])
    } else {
        None
    }
}

/// For a two-argument op: if the right side is the constant `identity`, the left side.
fn right_side(rebuilder: &Rebuilder, args: &[NodeId], identity: f32) -> Option<NodeId> {
    let is_identity = constant_floats(rebuilder, args[1]).is_some_and(|f| f.iter().all(|x| *x == identity));
    is_identity.then_some(args[0])
}

fn constant_floats(rebuilder: &Rebuilder, id: NodeId) -> Option<Vec<f32>> {
    match &rebuilder.new.node(id).kind {
        NodeKind::Const(v) => Some(v.floats()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    fn n(kind: NodeKind, ty: Type) -> Node {
        Node { kind, ty, origin: String::new() }
    }

    fn field_node(out: &Dag) -> &Node {
        out.node(out.surface.fields[0].node)
    }

    #[test]
    fn multiply_by_one_becomes_the_other_arg() {
        let mut dag = Dag::new("m");
        let x = dag.add(n(NodeKind::Uniform { name: "x".into(), value: Value::Float(0.3) }, Type::Float));
        let one = dag.add(n(NodeKind::Const(Value::Float(1.0)), Type::Float));
        let m = dag.add(n(NodeKind::Op { op: Op::Multiply, args: vec![x, one] }, Type::Float));
        dag.surface.fields.push(SurfaceField { name: "f".into(), ty: Type::Float, node: m });
        let out = super::run(&dag);
        assert!(matches!(field_node(&out).kind, NodeKind::Uniform { .. }));
    }

    #[test]
    fn multiply_by_one_keeps_a_broadcasting_op() {
        let mut dag = Dag::new("m");
        let x = dag.add(n(NodeKind::Uniform { name: "x".into(), value: Value::Float(0.3) }, Type::Float));
        let one = dag.add(n(NodeKind::Const(Value::Vector3([1.0; 3])), Type::Vector3));
        let m = dag.add(n(NodeKind::Op { op: Op::Multiply, args: vec![x, one] }, Type::Vector3));
        dag.surface.fields.push(SurfaceField { name: "f".into(), ty: Type::Vector3, node: m });
        let out = super::run(&dag);
        assert!(matches!(field_node(&out).kind, NodeKind::Op { op: Op::Multiply, .. }));
    }

    #[test]
    fn add_zero_becomes_the_other_arg() {
        let mut dag = Dag::new("m");
        let zero = dag.add(n(NodeKind::Const(Value::Float(0.0)), Type::Float));
        let x = dag.add(n(NodeKind::Uniform { name: "x".into(), value: Value::Float(0.3) }, Type::Float));
        let a = dag.add(n(NodeKind::Op { op: Op::Add, args: vec![zero, x] }, Type::Float));
        dag.surface.fields.push(SurfaceField { name: "f".into(), ty: Type::Float, node: a });
        let out = super::run(&dag);
        assert!(matches!(field_node(&out).kind, NodeKind::Uniform { .. }));
    }

    /// `x - 0` and `x / 1` are `x`; `0 - x` and `1 / x` are not.
    #[test]
    fn subtract_zero_and_divide_by_one_on_the_right_only() {
        for (op, identity) in [(Op::Subtract, 0.0), (Op::Divide, 1.0)] {
            for right in [true, false] {
                let mut dag = Dag::new("m");
                let x = dag.add(n(NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2));
                let c = dag.add(n(NodeKind::Const(Value::Vector2([identity; 2])), Type::Vector2));
                let args = if right { vec![x, c] } else { vec![c, x] };
                let m = dag.add(n(NodeKind::Op { op, args }, Type::Vector2));
                dag.surface.fields.push(SurfaceField { name: "f".into(), ty: Type::Vector2, node: m });
                let out = super::run(&dag);
                assert_eq!(matches!(field_node(&out).kind, NodeKind::Context(_)), right, "{op:?} right={right}");
            }
        }
    }

    #[test]
    fn mix_with_constant_factor_picks_a_side() {
        for (factor, expect) in [(0.0, "bg"), (1.0, "fg")] {
            let mut dag = Dag::new("m");
            let fg = dag.add(n(NodeKind::Uniform { name: "fg".into(), value: Value::Float(0.3) }, Type::Float));
            let bg = dag.add(n(NodeKind::Uniform { name: "bg".into(), value: Value::Float(0.6) }, Type::Float));
            let f = dag.add(n(NodeKind::Const(Value::Float(factor)), Type::Float));
            let m = dag.add(n(NodeKind::Op { op: Op::Mix, args: vec![fg, bg, f] }, Type::Float));
            dag.surface.fields.push(SurfaceField { name: "f".into(), ty: Type::Float, node: m });
            let out = super::run(&dag);
            let NodeKind::Uniform { name, .. } = &field_node(&out).kind else { panic!() };
            assert_eq!(name, expect);
        }
    }
}
