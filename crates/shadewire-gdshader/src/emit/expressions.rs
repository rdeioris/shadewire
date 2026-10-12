use shadewire::reflection::live::live_nodes;
use shadewire::{Dag, NodeId, NodeKind, Reflection};

use super::context::member_name;
use super::names::{CONTEXT, UniformNames};
use super::ops::{Arg, cast, emit_op, work_type};
use super::sample::sample_expr;
use super::types::{gd_type, literal};

/// The name of a node's value.
pub fn node_name(id: NodeId) -> String {
    format!("n{}", id.0)
}

/// One `{type} n{i} = {expr};` per live node, in node order.
/// Live uniforms are numbered in node order, exactly as reflection lists them.
pub fn node_lets(dag: &Dag, reflection: &Reflection, names: &UniformNames) -> Vec<String> {
    let live = live_nodes(dag);
    let mut uniform_index = 0;
    let mut lines = Vec::new();
    for (i, node) in dag.nodes.iter().enumerate() {
        if !live[i] {
            continue;
        }
        let id = NodeId(i as u32);
        let expr = match &node.kind {
            NodeKind::Uniform { .. } => {
                let name = names.name(uniform_index).to_string();
                uniform_index += 1;
                name
            }
            NodeKind::Context(read) => {
                cast(&format!("{CONTEXT}.{}", member_name(read, reflection)), read.ty(), node.ty)
            }
            NodeKind::Sample { texture, uv, default, grad } => {
                let t = reflection.textures.iter().find(|t| t.index == *texture).expect("live texture is reflected");
                let work = work_type(node.ty);
                let fallback = cast(&literal(default), default.ty(), work);
                let grad = grad.map(|[dx, dy]| (node_name(dx), node_name(dy)));
                let grad = grad.as_ref().map(|(dx, dy)| (dx.as_str(), dy.as_str()));
                cast(&sample_expr(t, &node_name(*uv), work, &fallback, grad), work, node.ty)
            }
            NodeKind::Const(_) | NodeKind::Op { .. } => pure_expr(dag, id),
        };
        lines.push(format!("{} {} = {expr};", gd_type(node.ty), node_name(id)));
    }
    lines
}

/// The expression of a node that needs no host data: a constant or an op.
/// Panics on uniforms, context reads and samples.
pub fn pure_expr(dag: &Dag, id: NodeId) -> String {
    let node = dag.node(id);
    match &node.kind {
        NodeKind::Const(value) => cast(&literal(value), value.ty(), node.ty),
        NodeKind::Op { op, args } => {
            let args: Vec<Arg> = args.iter().map(|a| (node_name(*a), dag.node(*a).ty)).collect();
            emit_op(op, &args, node.ty)
        }
        other => panic!("{other:?} reads host data"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::test_support::{add, reflection};
    use shadewire::*;

    #[test]
    fn uniforms_context_and_ops() {
        let mut dag = Dag::new("open_pbr_surface");
        let flag = add(&mut dag, NodeKind::Uniform { name: "flag".into(), value: Value::Boolean(true) }, Type::Boolean);
        add(&mut dag, NodeKind::Const(Value::Float(9.0)), Type::Float);
        let prop = ContextRead::GeomProp { name: "my-prop".into(), ty: Type::Float };
        let p = add(&mut dag, NodeKind::Context(prop), Type::Float);
        let w = add(&mut dag, NodeKind::Uniform { name: "my prop".into(), value: Value::Float(0.5) }, Type::Float);
        let gt = add(&mut dag, NodeKind::Op { op: Op::IfGreater, args: vec![p, w, p, w] }, Type::Float);
        let not = add(&mut dag, NodeKind::Op { op: Op::Not, args: vec![flag] }, Type::Boolean);
        dag.surface.fields.push(SurfaceField { name: "a".into(), ty: Type::Float, node: gt });
        dag.surface.fields.push(SurfaceField { name: "b".into(), ty: Type::Boolean, node: not });
        let r = reflection(&dag);
        assert_eq!(
            node_lets(&dag, &r, &UniformNames::new(&r)),
            vec![
                "bool n0 = flag;",
                "float n2 = ctx.geomprop_my_prop;",
                "float n3 = my_prop;",
                "float n4 = ((n2 > n3) ? n2 : n3);",
                "bool n5 = (!n0);",
            ]
        );
    }
}
