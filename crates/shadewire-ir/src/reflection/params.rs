use std::collections::BTreeMap;

use serde::Serialize;

use crate::classify::node_kinds;
use crate::{Dag, NodeKind, ParamKind, Type, Value};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ParamReflection {
    #[serde(rename = "type")]
    pub ty: Type,
    /// "constant", "uniform" or "varying".
    pub kind: &'static str,
    pub value: Option<Value>,
    /// Some only when the field node itself is a Uniform.
    pub uniform: Option<String>,
}

pub fn params(dag: &Dag) -> BTreeMap<String, ParamReflection> {
    let kinds = node_kinds(dag);
    let mut out = BTreeMap::new();
    for field in &dag.surface.fields {
        let (kind, value) = match &kinds[field.node.index()] {
            ParamKind::Constant(v) => ("constant", Some(v.clone())),
            ParamKind::Uniform => ("uniform", None),
            ParamKind::Varying => ("varying", None),
        };
        let uniform = match &dag.node(field.node).kind {
            NodeKind::Uniform { name, .. } => Some(name.clone()),
            _ => None,
        };
        out.insert(field.name.clone(), ParamReflection { ty: field.ty, kind, value, uniform });
    }
    out
}
