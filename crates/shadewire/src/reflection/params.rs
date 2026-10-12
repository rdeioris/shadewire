use std::collections::BTreeMap;

use serde::Serialize;

use crate::classify::node_kinds;
use crate::{Dag, NodeKind, ParamKind, Type, Value};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ParamReflection {
    #[serde(rename = "type")]
    pub ty: Type,
    /// "constant"; "uniform" when the field is a uniform itself, so a host may
    /// pass the uniform's value straight through; "uniform_expr" when it is
    /// computed from uniforms only (a colorspace conversion, math over
    /// interface values), so the host must run the shader or the IR to get it;
    /// "varying" when it changes per fragment.
    pub kind: &'static str,
    pub value: Option<Value>,
    /// Some only when the field node itself is a Uniform.
    pub uniform: Option<String>,
}

pub fn params(dag: &Dag) -> BTreeMap<String, ParamReflection> {
    let kinds = node_kinds(dag);
    let mut out = BTreeMap::new();
    for field in &dag.surface.fields {
        let uniform = match &dag.node(field.node).kind {
            NodeKind::Uniform { name, .. } => Some(name.clone()),
            _ => None,
        };
        let (kind, value) = match (&kinds[field.node.index()], &uniform) {
            (ParamKind::Constant(v), _) => ("constant", Some(v.clone())),
            (ParamKind::Uniform, Some(_)) => ("uniform", None),
            (ParamKind::Uniform, None) => ("uniform_expr", None),
            (ParamKind::Varying, _) => ("varying", None),
        };
        out.insert(field.name.clone(), ParamReflection { ty: field.ty, kind, value, uniform });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::params;
    use crate::*;

    fn node(kind: NodeKind) -> Node {
        Node { kind, ty: Type::Float, origin: String::new() }
    }

    #[test]
    fn only_a_uniform_field_is_uniform_and_math_over_one_is_uniform_expr() {
        let mut dag = Dag::new("m");
        let u = dag.add(node(NodeKind::Uniform { name: "k".into(), value: Value::Float(1.0) }));
        let c = dag.add(node(NodeKind::Const(Value::Float(2.0))));
        let sum = dag.add(node(NodeKind::Op { op: Op::Add, args: vec![u, c] }));
        dag.surface.fields.push(SurfaceField { name: "plain".into(), ty: Type::Float, node: u });
        dag.surface.fields.push(SurfaceField { name: "computed".into(), ty: Type::Float, node: sum });
        let p = params(&dag);
        assert_eq!((p["plain"].kind, p["plain"].uniform.as_deref()), ("uniform", Some("k")));
        assert_eq!((p["computed"].kind, p["computed"].uniform.as_deref()), ("uniform_expr", None));
    }
}
