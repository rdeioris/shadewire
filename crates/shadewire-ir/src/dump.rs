use crate::{Dag, Node, NodeKind, Value};

impl Dag {
    /// One line per node, then one line per surface field and approximation.
    pub fn dump(&self) -> String {
        let mut out = format!("model {}\n", self.surface.model);
        for (index, node) in self.nodes.iter().enumerate() {
            out.push_str(&format!("%{index} {} = {}", node.ty.mtlx_name(), body(node)));
            if !node.origin.is_empty() {
                out.push_str(&format!("    # {}", node.origin));
            }
            out.push('\n');
        }
        for field in &self.surface.fields {
            out.push_str(&format!("{} = %{}\n", field.name, field.node.index()));
        }
        for approximation in &self.approximations {
            out.push_str(&format!("approx {approximation}\n"));
        }
        out
    }
}

fn body(node: &Node) -> String {
    match &node.kind {
        NodeKind::Const(value) => format!("const {}", value_text(value)),
        NodeKind::Uniform { name, value } => format!("uniform {name} = {}", value_text(value)),
        NodeKind::Context(read) => format!("context {}", read.reflection_name()),
        NodeKind::Sample { texture, uv, grad: None, .. } => format!("sample t{texture}(%{})", uv.index()),
        NodeKind::Sample { texture, uv, grad: Some([dx, dy]), .. } => {
            format!("sample t{texture}(%{}) grad(%{}, %{})", uv.index(), dx.index(), dy.index())
        }
        NodeKind::Op { op, args } => {
            let args: Vec<String> = args.iter().map(|a| format!("%{}", a.index())).collect();
            format!("{:?}({})", op, args.join(", "))
        }
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        _ => {
            let parts: Vec<String> = value.floats().iter().map(|f| f.to_string()).collect();
            parts.join(", ")
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn vectors_sampling_and_uniforms_dump_exactly() {
        let mut dag = Dag::new("open_pbr_surface");
        let t = dag.add_texture(TextureDesc { file: "a.png".into(), colorspace: None, address_u: AddressMode::Periodic, address_v: AddressMode::Periodic, filter: FilterMode::Linear });
        dag.add(Node { kind: NodeKind::Const(Value::Vector3([1.0, 0.5, 0.0])), ty: Type::Vector3, origin: "n1".into() });
        dag.add(Node { kind: NodeKind::Uniform { name: "tint".into(), value: Value::Color3([0.2, 0.3, 0.4]) }, ty: Type::Color3, origin: String::new() });
        let uv = dag.add(Node { kind: NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), ty: Type::Vector2, origin: String::new() });
        let s = dag.add(Node { kind: NodeKind::Sample { texture: t, uv, default: Value::Float(0.0), grad: None }, ty: Type::Float, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "out".into(), ty: Type::Float, node: s });
        assert_eq!(
            dag.dump(),
            "model open_pbr_surface\n%0 vector3 = const 1, 0.5, 0    # n1\n%1 color3 = uniform tint = 0.2, 0.3, 0.4\n%2 vector2 = context texcoord_0\n%3 float = sample t0(%2)\nout = %3\n"
        );
    }

    #[test]
    fn sample_gradients_dump_after_the_uv() {
        let mut dag = Dag::new("m");
        let t = dag.add_texture(TextureDesc { file: "a.png".into(), colorspace: None, address_u: AddressMode::Periodic, address_v: AddressMode::Periodic, filter: FilterMode::Linear });
        let uv = dag.add(Node { kind: NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), ty: Type::Vector2, origin: String::new() });
        let s = dag.add(Node { kind: NodeKind::Sample { texture: t, uv, default: Value::Float(0.0), grad: Some([uv, uv]) }, ty: Type::Float, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "out".into(), ty: Type::Float, node: s });
        assert!(dag.dump().contains("%1 float = sample t0(%0) grad(%0, %0)
"), "{}", dag.dump());
    }

    #[test]
    fn dump_is_one_line_per_node() {
        let mut dag = Dag::new("open_pbr_surface");
        let c = dag.add(Node { kind: NodeKind::Const(Value::Float(0.5)), ty: Type::Float, origin: "n1".into() });
        let s = dag.add(Node { kind: NodeKind::Op { op: Op::Sin, args: vec![c] }, ty: Type::Float, origin: "n2".into() });
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: s });
        assert_eq!(dag.dump(), "model open_pbr_surface\n%0 float = const 0.5    # n1\n%1 float = Sin(%0)    # n2\nbase_weight = %1\n");
    }
}
