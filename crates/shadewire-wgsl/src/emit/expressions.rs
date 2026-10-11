use shadewire_ir::reflection::live::live_nodes;
use shadewire_ir::{ContextRead, Dag, NodeId, NodeKind, Reflection, Type};

use super::names::UniformNames;
use super::ops::{cast, emit_op, work_type, Arg};
use super::sample::sample_expr;
use super::types::{literal, wgsl_type};
use super::EmitOptions;

/// The WGSL name of a node's value.
pub fn node_name(id: NodeId) -> String {
    format!("n{}", id.0)
}

/// One `let n{i}: {type} = {expr};` per live node, in node order.
/// Live uniforms are numbered in node order, exactly as reflection lists them.
pub fn node_lets(dag: &Dag, reflection: &Reflection, options: &EmitOptions) -> Vec<String> {
    let live = live_nodes(dag);
    let members = UniformNames::new(reflection);
    let mut uniform_index = 0;
    let mut lines = Vec::new();
    for (i, node) in dag.nodes.iter().enumerate() {
        if !live[i] {
            continue;
        }
        let id = NodeId(i as u32);
        let expr = match &node.kind {
            NodeKind::Uniform { .. } => {
                let member = members.member(uniform_index);
                uniform_index += 1;
                match node.ty {
                    Type::Boolean => format!("(sw_u.{member} != 0u)"),
                    _ => format!("sw_u.{member}"),
                }
            }
            NodeKind::Context(read) => cast(&format!("{}()", context_function(read, reflection)), read.ty(), node.ty),
            NodeKind::Sample { texture, uv, default } => {
                let t = reflection.textures.iter().find(|t| t.index == *texture).expect("live texture is reflected");
                let work = work_type(node.ty);
                let fallback = cast(&literal(default), default.ty(), work);
                let fetch = sample_expr(t, &node_name(*uv), work, &fallback, options.flip_v);
                cast(&fetch, work, node.ty)
            }
            NodeKind::Const(_) | NodeKind::Op { .. } => pure_expr(dag, id),
        };
        lines.push(let_line(dag, id, &expr));
    }
    lines
}

/// Geomprops call the function reflection gave them (sanitised and unique);
/// every other read calls `ContextRead::function_name`.
fn context_function(read: &ContextRead, reflection: &Reflection) -> String {
    match read {
        ContextRead::GeomProp { name, .. } => {
            let g = reflection.context.geomprops.iter().find(|g| g.name == *name).expect("live geomprop is reflected");
            g.function.clone()
        }
        other => other.function_name(),
    }
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

/// `let n{i}: {type} = {expr};`
pub fn let_line(dag: &Dag, id: NodeId, expr: &str) -> String {
    format!("let {}: {} = {expr};", node_name(id), wgsl_type(dag.node(id).ty))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::test_support::{add, reflection};
    use shadewire_ir::*;

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
            node_lets(&dag, &r, &EmitOptions::default()),
            vec![
                "let n0: bool = (sw_u.flag != 0u);",
                "let n2: f32 = mx_geomprop_my_prop();",
                "let n3: f32 = sw_u.my_prop;",
                "let n4: f32 = select(n3, n2, (n2 > n3));",
                "let n5: bool = (!n0);",
            ]
        );
    }

    #[test]
    fn same_name_uniforms_read_their_own_members() {
        let mut dag = Dag::new("m");
        let a = add(&mut dag, NodeKind::Uniform { name: "w".into(), value: Value::Float(0.5) }, Type::Float);
        let b = add(&mut dag, NodeKind::Uniform { name: "w".into(), value: Value::Float(0.25) }, Type::Float);
        let s = add(&mut dag, NodeKind::Op { op: Op::Add, args: vec![a, b] }, Type::Float);
        dag.surface.fields.push(SurfaceField { name: "a".into(), ty: Type::Float, node: s });
        let lines = node_lets(&dag, &reflection(&dag), &EmitOptions::default());
        assert_eq!(lines[..2], ["let n0: f32 = sw_u.w;", "let n1: f32 = sw_u.w_2;"]);
    }

    #[test]
    fn samples_use_the_reflected_texture() {
        let mut dag = Dag::new("m");
        let mk = |file: &str, u| TextureDesc { file: file.into(), colorspace: None, address_u: u, address_v: AddressMode::Periodic, filter: FilterMode::Linear };
        dag.add_texture(mk("dead.png", AddressMode::Periodic));
        let t = dag.add_texture(mk("a.png", AddressMode::Constant));
        let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
        let s = add(&mut dag, NodeKind::Sample { texture: t, uv, default: Value::Float(0.5) }, Type::Color3);
        dag.surface.fields.push(SurfaceField { name: "c".into(), ty: Type::Color3, node: s });
        let lines = node_lets(&dag, &reflection(&dag), &EmitOptions::default());
        assert_eq!(
            lines[1],
            "let n1: vec3<f32> = select(textureSample(sw_t1, sw_s1, vec2<f32>(n0.x, 1.0 - n0.y)).rgb, vec3<f32>(0.5), n0.x < 0.0 || n0.x > 1.0);"
        );
    }

    #[test]
    fn transforms_call_the_host() {
        let mut dag = Dag::new("m");
        let n = add(&mut dag, NodeKind::Context(ContextRead::Normal(Space::Object)), Type::Vector3);
        let op = Op::Transform { kind: TransformKind::Normal, from: Space::Object, to: Space::World };
        let t = add(&mut dag, NodeKind::Op { op, args: vec![n] }, Type::Vector3);
        dag.surface.fields.push(SurfaceField { name: "n".into(), ty: Type::Vector3, node: t });
        let r = reflection(&dag);
        assert_eq!(r.context.transforms, ["transform_normal_object_to_world"]);
        assert_eq!(
            node_lets(&dag, &r, &EmitOptions::default()),
            ["let n0: vec3<f32> = mx_normal_object();", "let n1: vec3<f32> = normalize(mx_transform_normal_object_to_world(n0));"]
        );
    }
}
