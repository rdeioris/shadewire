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
        NodeKind::Sample { texture, uv, default } => {
            format!("sample t{texture}(%{}) default {}", uv.index(), value_text(default))
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
            let floats = value.floats();
            if floats.len() == 1 {
                format!("{}", floats[0])
            } else {
                let parts: Vec<String> = floats.iter().map(|f| f.to_string()).collect();
                format!("[{}]", parts.join(", "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn dump_is_one_line_per_node() {
        let mut dag = Dag::new("open_pbr_surface");
        let c = dag.add(Node { kind: NodeKind::Const(Value::Float(0.5)), ty: Type::Float, origin: "n1".into() });
        let s = dag.add(Node { kind: NodeKind::Op { op: Op::Sin, args: vec![c] }, ty: Type::Float, origin: "n2".into() });
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: s });
        assert_eq!(dag.dump(), "model open_pbr_surface\n%0 float = const 0.5    # n1\n%1 float = Sin(%0)    # n2\nbase_weight = %1\n");
    }
}
