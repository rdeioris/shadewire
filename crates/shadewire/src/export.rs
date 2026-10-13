//! The IR as a flat, serializable document, for hosts in other languages (the Blender add-on reads it as JSON
//! from `shadewire ir --json`).
//!
//! Every node is an object with `kind` and `type` (MaterialX type names), in dependency order; arguments are
//! indices into `nodes`. Ops are named in snake case (`safe_power`, `if_greater_eq`, `worley_noise3d`), with
//! `index` on `extract`, `transform_kind`/`from`/`to` on `transform`, and `tile` on the hextile ops. Values are
//! numbers, lists of numbers (vectors, colors), or booleans.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{ClosureGraph, ContextRead, Dag, NodeKind, Op, TextureDesc, Type, Value};

/// The version of the document's layout; it changes when a field changes meaning.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ExportValue {
    Boolean(bool),
    Integer(i32),
    Floats(Vec<f32>),
}

impl From<&Value> for ExportValue {
    fn from(value: &Value) -> ExportValue {
        match value {
            Value::Boolean(b) => ExportValue::Boolean(*b),
            Value::Integer(i) => ExportValue::Integer(*i),
            other => ExportValue::Floats(other.floats()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExportNode {
    pub kind: &'static str,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<ExportValue>,
    /// A uniform's name, or a geomprop's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// A context read: `position`, `normal`, `tangent`, `bitangent`, `texcoord`, `geomcolor`, `geomprop`,
    /// `view_direction`, `time`, `frame`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space: Option<&'static str>,
    /// A texcoord or geomcolor set; an `extract` channel; a hextile tile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<u32>,
    /// `point`, `vector` or `normal`, for `transform`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<&'static str>,
    /// A sample's texture, an index into `textures`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture: Option<usize>,
    /// A sample's texture coordinate node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uv: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExportField {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    pub node: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExportDocument {
    pub format: &'static str,
    pub version: u32,
    pub model: String,
    pub nodes: Vec<ExportNode>,
    pub textures: Vec<TextureDesc>,
    pub fields: Vec<ExportField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closure: Option<ClosureGraph>,
    pub geomprop_defaults: BTreeMap<String, ExportValue>,
    pub approximations: Vec<String>,
}

/// `SafePower` as `safe_power`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn empty(kind: &'static str, ty: Type) -> ExportNode {
    ExportNode {
        kind,
        ty,
        value: None,
        name: None,
        read: None,
        space: None,
        index: None,
        op: None,
        args: Vec::new(),
        transform_kind: None,
        from: None,
        to: None,
        texture: None,
        uv: None,
        origin: None,
    }
}

fn context_node(read: &ContextRead, ty: Type) -> ExportNode {
    let mut node = empty("context", ty);
    let (name, space, index, geomprop) = match read {
        ContextRead::Position(s) => ("position", Some(s.name()), None, None),
        ContextRead::Normal(s) => ("normal", Some(s.name()), None, None),
        ContextRead::Tangent(s) => ("tangent", Some(s.name()), None, None),
        ContextRead::Bitangent(s) => ("bitangent", Some(s.name()), None, None),
        ContextRead::ViewDirection(s) => ("view_direction", Some(s.name()), None, None),
        ContextRead::TexCoord { index, .. } => ("texcoord", None, Some(*index), None),
        ContextRead::GeomColor { index, .. } => ("geomcolor", None, Some(*index), None),
        ContextRead::GeomProp { name, .. } => ("geomprop", None, None, Some(name.clone())),
        ContextRead::Time => ("time", None, None, None),
        ContextRead::Frame => ("frame", None, None, None),
    };
    node.read = Some(name);
    node.space = space;
    node.index = index;
    node.name = geomprop;
    node
}

fn op_node(op: &Op, args: &[crate::NodeId], ty: Type) -> ExportNode {
    let mut node = empty("op", ty);
    node.args = args.iter().map(|a| a.0).collect();
    let name = match op {
        Op::Extract(i) => {
            node.index = Some(*i);
            "extract".to_string()
        }
        Op::Transform { kind, from, to } => {
            node.transform_kind = Some(kind.name());
            node.from = Some(from.name());
            node.to = Some(to.name());
            "transform".to_string()
        }
        Op::HextileCoord(t) => {
            node.index = Some(u32::from(*t));
            "hextile_coord".to_string()
        }
        Op::HextileGrad(t) => {
            node.index = Some(u32::from(*t));
            "hextile_grad".to_string()
        }
        other => snake(&format!("{other:?}")),
    };
    node.op = Some(name);
    node
}

/// The document for `dag`.
pub fn export(dag: &Dag) -> ExportDocument {
    let nodes = dag
        .nodes
        .iter()
        .map(|n| {
            let mut out = match &n.kind {
                NodeKind::Const(v) => ExportNode { value: Some(v.into()), ..empty("const", n.ty) },
                NodeKind::Uniform { name, value } => {
                    ExportNode { value: Some(value.into()), name: Some(name.clone()), ..empty("uniform", n.ty) }
                }
                NodeKind::Context(read) => context_node(read, n.ty),
                NodeKind::Sample { texture, uv, default, grad: _ } => ExportNode {
                    texture: Some(*texture),
                    uv: Some(uv.0),
                    value: Some(default.into()),
                    ..empty("sample", n.ty)
                },
                NodeKind::Op { op, args } => op_node(op, args, n.ty),
            };
            out.origin = (!n.origin.is_empty()).then(|| n.origin.clone());
            out
        })
        .collect();
    ExportDocument {
        format: "shadewire-ir",
        version: FORMAT_VERSION,
        model: dag.surface.model.clone(),
        nodes,
        textures: dag.textures.clone(),
        fields: dag
            .surface
            .fields
            .iter()
            .map(|f| ExportField { name: f.name.clone(), ty: f.ty, node: f.node.0 })
            .collect(),
        closure: dag.surface.closure.clone(),
        geomprop_defaults: dag.geomprop_defaults.iter().map(|(k, v)| (k.clone(), v.into())).collect(),
        approximations: dag.approximations.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn ops_are_snake_case_with_their_parameters() {
        let mut dag = Dag::new("open_pbr_surface");
        let c = dag.add(Node {
            kind: NodeKind::Const(Value::Color3([1.0, 0.5, 0.25])),
            ty: Type::Color3,
            origin: String::new(),
        });
        let e = dag.add(Node {
            kind: NodeKind::Op { op: Op::Extract(1), args: vec![c] },
            ty: Type::Float,
            origin: String::new(),
        });
        let p = dag.add(Node {
            kind: NodeKind::Op { op: Op::SafePower, args: vec![e, e] },
            ty: Type::Float,
            origin: "g/pow".into(),
        });
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: p });
        let json = serde_json::to_string(&export(&dag).nodes).unwrap();
        assert_eq!(
            json,
            r#"[{"kind":"const","type":"color3","value":[1.0,0.5,0.25]},{"kind":"op","type":"float","index":1,"op":"extract","args":[0]},{"kind":"op","type":"float","op":"safe_power","args":[1,1],"origin":"g/pow"}]"#
        );
    }

    #[test]
    fn context_reads_name_their_space_and_set() {
        let n = context_node(&ContextRead::Normal(Space::World), Type::Vector3);
        assert_eq!((n.read, n.space), (Some("normal"), Some("world")));
        let t = context_node(&ContextRead::TexCoord { index: 1, ty: Type::Vector2 }, Type::Vector2);
        assert_eq!((t.read, t.index), (Some("texcoord"), Some(1)));
    }
}
