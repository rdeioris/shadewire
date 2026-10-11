use crate::interpret::{ContextSource, TextureSource};
use crate::ops::broadcast::broadcast;
use crate::ops::eval_op;
use crate::{Dag, NodeKind, Value};

pub fn eval_nodes(dag: &Dag, ctx: &mut dyn ContextSource, textures: &dyn TextureSource) -> Vec<Value> {
    let mut values: Vec<Value> = Vec::with_capacity(dag.nodes.len());
    for node in &dag.nodes {
        let value = match &node.kind {
            NodeKind::Const(v) => v.clone(),
            NodeKind::Uniform { value, .. } => value.clone(),
            NodeKind::Context(read) => ctx.read(read),
            // The interpreter has no neighbouring fragments: gradients pick no mip level.
            NodeKind::Sample { texture, uv, default, .. } => {
                let uv_floats = values[uv.index()].floats();
                match textures.sample(&dag.textures[*texture], [uv_floats[0], uv_floats[1]]) {
                    Some(rgba) => Value::from_floats(node.ty, &rgba[..node.ty.component_count()]),
                    None => default.clone(),
                }
            }
            NodeKind::Op { op, args } => {
                let arg_values: Vec<Value> = args.iter().map(|a| values[a.index()].clone()).collect();
                eval_op(op, &arg_values, node.ty, ctx)
            }
        };
        values.push(broadcast(&value, node.ty));
    }
    values
}

pub fn eval_surface(dag: &Dag, ctx: &mut dyn ContextSource, textures: &dyn TextureSource) -> Vec<(String, Value)> {
    let values = eval_nodes(dag, ctx, textures);
    dag.surface.fields.iter().map(|f| (f.name.clone(), values[f.node.index()].clone())).collect()
}

#[cfg(test)]
mod tests {
    use crate::interpret::*;
    use crate::*;

    struct UvOnly([f32; 2]);
    impl ContextSource for UvOnly {
        fn read(&mut self, read: &ContextRead) -> Value {
            match read {
                ContextRead::TexCoord { .. } => Value::Vector2(self.0),
                other => Value::zero(other.ty()),
            }
        }
        fn transform(&mut self, _: TransformKind, _: Space, _: Space, v: [f32; 3]) -> [f32; 3] {
            v
        }
    }

    #[test]
    fn evaluates_a_small_graph() {
        let mut dag = Dag::new("open_pbr_surface");
        let uv = dag.add(Node { kind: NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), ty: Type::Vector2, origin: String::new() });
        let u = dag.add(Node { kind: NodeKind::Op { op: Op::Extract(0), args: vec![uv] }, ty: Type::Float, origin: String::new() });
        let two = dag.add(Node { kind: NodeKind::Const(Value::Float(2.0)), ty: Type::Float, origin: String::new() });
        let m = dag.add(Node { kind: NodeKind::Op { op: Op::Multiply, args: vec![u, two] }, ty: Type::Float, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: m });
        let out = eval_surface(&dag, &mut UvOnly([0.25, 0.5]), &NoTextures);
        assert_eq!(out, vec![("base_weight".to_string(), Value::Float(0.5))]);
    }

    #[test]
    fn constants_broadcast_to_node_type() {
        let mut dag = Dag::new("open_pbr_surface");
        let c = dag.add(Node { kind: NodeKind::Const(Value::Float(0.5)), ty: Type::Color3, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: c });
        let out = eval_surface(&dag, &mut UvOnly([0.0, 0.0]), &NoTextures);
        assert_eq!(out, vec![("base_color".to_string(), Value::Color3([0.5, 0.5, 0.5]))]);
    }
}
