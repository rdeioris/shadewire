use shadewire_ir::{Dag, NodeId, NodeKind, Type};

use super::expressions::{let_line, node_name, pure_expr};
use super::ops::cast;

/// `fn {name}() -> vec4<f32>` computing one node from constants, widened to vec4 with 0 fill.
/// For the GPU parity test: panics on uniforms, context reads and samples.
pub fn probe_function(dag: &Dag, node: NodeId, name: &str) -> String {
    let mut needed = vec![false; dag.nodes.len()];
    let mut stack = vec![node];
    while let Some(id) = stack.pop() {
        if !needed[id.index()] {
            needed[id.index()] = true;
            stack.extend(dag.args(id));
        }
    }
    let mut out = format!("fn {name}() -> vec4<f32> {{\n");
    for (i, n) in dag.nodes.iter().enumerate() {
        if !needed[i] {
            continue;
        }
        if !matches!(n.kind, NodeKind::Const(_) | NodeKind::Op { .. }) {
            panic!("probe {name}: node {i} reads host data ({:?})", n.kind);
        }
        let id = NodeId(i as u32);
        out.push_str(&format!("    {}\n", let_line(dag, id, &pure_expr(dag, id))));
    }
    out.push_str(&format!("    return {};\n}}\n", widen(&node_name(node), dag.node(node).ty)));
    out
}

/// A value as vec4<f32>, missing components 0.
fn widen(expr: &str, ty: Type) -> String {
    match ty.component_count() {
        1 => format!("vec4<f32>({}, 0.0, 0.0, 0.0)", cast(expr, ty, Type::Float)),
        2 => format!("vec4<f32>({expr}, 0.0, 0.0)"),
        3 => format!("vec4<f32>({expr}, 0.0)"),
        _ => expr.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::test_support::{add, naga_validates};
    use shadewire_ir::*;

    #[test]
    fn ancestors_only_widened_to_vec4() {
        let mut dag = Dag::new("m");
        let a = add(&mut dag, NodeKind::Const(Value::Float(2.0)), Type::Float);
        add(&mut dag, NodeKind::Const(Value::Float(7.0)), Type::Float);
        let s = add(&mut dag, NodeKind::Op { op: Op::Sqrt, args: vec![a] }, Type::Float);
        let text = probe_function(&dag, s, "probe_2");
        assert_eq!(
            text,
            "fn probe_2() -> vec4<f32> {\n    let n0: f32 = 2.0;\n    let n2: f32 = sqrt(n0);\n    return vec4<f32>(n2, 0.0, 0.0, 0.0);\n}\n"
        );
        naga_validates(&text);
    }

    #[test]
    fn widths_and_scalar_kinds() {
        let mut dag = Dag::new("m");
        let c = add(&mut dag, NodeKind::Const(Value::Color3([0.1, 0.2, 0.3])), Type::Color3);
        let i = add(&mut dag, NodeKind::Const(Value::Integer(3)), Type::Integer);
        let b = add(&mut dag, NodeKind::Op { op: Op::Greater, args: vec![i, i] }, Type::Boolean);
        assert!(probe_function(&dag, c, "p").contains("return vec4<f32>(n0, 0.0);"));
        assert!(probe_function(&dag, i, "p").contains("return vec4<f32>(f32(n1), 0.0, 0.0, 0.0);"));
        let text = probe_function(&dag, b, "p");
        assert!(text.contains("return vec4<f32>(f32(n2), 0.0, 0.0, 0.0);"), "{text}");
        naga_validates(&text);
    }

    #[test]
    fn non_finite_results_still_validate() {
        let mut dag = Dag::new("m");
        let one = add(&mut dag, NodeKind::Const(Value::Float(1.0)), Type::Float);
        let zero = add(&mut dag, NodeKind::Const(Value::Float(0.0)), Type::Float);
        let neg = add(&mut dag, NodeKind::Const(Value::Float(-1.0)), Type::Float);
        let inf = add(&mut dag, NodeKind::Op { op: Op::Divide, args: vec![one, zero] }, Type::Float);
        let nan = add(&mut dag, NodeKind::Op { op: Op::Sqrt, args: vec![neg] }, Type::Float);
        let low = add(&mut dag, NodeKind::Op { op: Op::Clamp, args: vec![one, one, zero] }, Type::Float);
        let sum = add(&mut dag, NodeKind::Op { op: Op::Add, args: vec![inf, nan] }, Type::Float);
        let all = add(&mut dag, NodeKind::Op { op: Op::Add, args: vec![sum, low] }, Type::Float);
        naga_validates(&probe_function(&dag, all, "p"));
    }

    #[test]
    #[should_panic(expected = "probe")]
    fn uniforms_are_refused() {
        let mut dag = Dag::new("m");
        let u = add(&mut dag, NodeKind::Uniform { name: "w".into(), value: Value::Float(1.0) }, Type::Float);
        probe_function(&dag, u, "p");
    }
}
