mod context;
pub mod live;
mod params;
mod textures;
pub mod uniforms;

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{Dag, Error, NodeKind, Type, Value, structure_hash};

pub use context::{ContextEntry, ContextReflection, GeomPropReflection};
pub use params::ParamReflection;
pub use textures::TextureReflection;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BindingOptions {
    pub group: u32,
    pub first_binding: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourceInfo {
    pub source: String,
    pub mtlx_version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UniformReflection {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    pub offset: u32,
    pub size: u32,
    pub default: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Reflection {
    pub shadewire: String,
    pub source: String,
    pub mtlx_version: String,
    pub model: String,
    pub structure_hash: String,
    pub params: BTreeMap<String, ParamReflection>,
    pub uniforms: Vec<UniformReflection>,
    pub uniform_block_size: u32,
    pub uniform_binding: Option<u32>,
    pub group: u32,
    pub textures: Vec<TextureReflection>,
    pub context: ContextReflection,
    pub displacement: Option<String>,
    pub approximations: Vec<String>,
}

/// What a host must bind and provide for `dag`. Fails when two context reads
/// share a host function but not a type.
pub fn reflect(dag: &Dag, bindings: &BindingOptions, source: &SourceInfo) -> Result<Reflection, Error> {
    let live = live::live_nodes(dag);
    let mut members: Vec<(String, Type)> = Vec::new();
    let mut defaults: Vec<Value> = Vec::new();
    for (i, node) in dag.nodes.iter().enumerate() {
        if !live[i] {
            continue;
        }
        if let NodeKind::Uniform { name, value } = &node.kind {
            members.push((name.clone(), node.ty));
            defaults.push(value.clone());
        }
    }
    let layout = uniforms::pack(&members);
    let uniform_list: Vec<UniformReflection> = members
        .iter()
        .zip(defaults)
        .zip(&layout.offsets)
        .map(|(((name, ty), default), offset)| UniformReflection {
            name: name.clone(),
            ty: *ty,
            offset: *offset,
            size: uniforms::wgsl_layout(*ty).1,
            default,
        })
        .collect();
    let has_uniforms = !uniform_list.is_empty();
    let first_texture = bindings.first_binding + has_uniforms as u32;

    let mut approximations = dag.approximations.clone();
    for note in textures::approximations(dag, &live) {
        if !approximations.contains(&note) {
            approximations.push(note);
        }
    }

    Ok(Reflection {
        shadewire: env!("CARGO_PKG_VERSION").to_string(),
        source: source.source.clone(),
        mtlx_version: source.mtlx_version.clone(),
        model: dag.surface.model.clone(),
        structure_hash: structure_hash(dag),
        params: params::params(dag),
        uniforms: uniform_list,
        uniform_block_size: layout.block_size,
        uniform_binding: has_uniforms.then_some(bindings.first_binding),
        group: bindings.group,
        textures: textures::textures(dag, &live, first_texture),
        context: context::context(dag, &live)?,
        displacement: None,
        approximations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    fn node(kind: NodeKind, ty: Type) -> Node {
        Node { kind, ty, origin: String::new() }
    }

    fn info() -> SourceInfo {
        SourceInfo { source: "a.mtlx".into(), mtlx_version: "1.39".into() }
    }

    fn sample_dag(with_uniform: bool) -> Dag {
        let mut dag = Dag::new("open_pbr_surface");
        let tex = dag.add_texture(TextureDesc {
            file: "a.png".into(),
            colorspace: None,
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Periodic,
            filter: FilterMode::Linear,
        });
        let c = dag.add(node(NodeKind::Const(Value::Float(0.5)), Type::Float));
        let uv = dag.add(node(NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2));
        let s =
            dag.add(node(NodeKind::Sample { texture: tex, uv, default: Value::Float(0.0), grad: None }, Type::Float));
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: c });
        dag.surface.fields.push(SurfaceField { name: "specular_weight".into(), ty: Type::Float, node: s });
        if with_uniform {
            let u =
                dag.add(node(NodeKind::Uniform { name: "coat_weight".into(), value: Value::Float(0.25) }, Type::Float));
            dag.surface.fields.push(SurfaceField { name: "coat_weight".into(), ty: Type::Float, node: u });
        }
        dag
    }

    #[test]
    fn reflects_kinds_and_bindings() {
        let dag = sample_dag(true);
        let r = reflect(&dag, &BindingOptions { group: 1, first_binding: 4 }, &info()).unwrap();
        assert_eq!(r.params["base_weight"].kind, "constant");
        assert_eq!(r.params["base_weight"].value, Some(Value::Float(0.5)));
        assert_eq!(r.params["coat_weight"].kind, "uniform");
        assert_eq!(r.params["coat_weight"].uniform, Some("coat_weight".to_string()));
        assert_eq!(r.params["specular_weight"].kind, "varying");
        assert_eq!(r.context.varyings, vec![ContextEntry { name: "texcoord_0".into(), ty: Type::Vector2 }]);
        assert_eq!(r.group, 1);
        assert_eq!(r.uniform_binding, Some(4));
        assert_eq!(r.uniform_block_size, 16);
        assert_eq!(r.uniforms.len(), 1);
        assert_eq!(r.uniforms[0].offset, 0);
        assert_eq!(r.uniforms[0].default, Value::Float(0.25));
        assert_eq!(r.textures[0].binding, 5);
        assert_eq!(r.textures[0].sampler_binding, 6);
        assert_eq!(r.displacement, None);
        assert_eq!(r.source, "a.mtlx");
        assert_eq!(r.structure_hash, structure_hash(&dag));
    }

    #[test]
    fn unreachable_nodes_are_not_reported() {
        let mut dag = sample_dag(false);
        let tex = dag.add_texture(TextureDesc {
            file: "dead.png".into(),
            colorspace: None,
            address_u: AddressMode::Clamp,
            address_v: AddressMode::Clamp,
            filter: FilterMode::Cubic,
        });
        dag.add(node(NodeKind::Uniform { name: "dead_uniform".into(), value: Value::Float(1.0) }, Type::Float));
        dag.add(node(NodeKind::Context(ContextRead::Time), Type::Float));
        let uv = dag.add(node(NodeKind::Const(Value::Vector2([0.0; 2])), Type::Vector2));
        dag.add(node(NodeKind::Sample { texture: tex, uv, default: Value::Float(0.0), grad: None }, Type::Float));
        let r = reflect(&dag, &BindingOptions { group: 0, first_binding: 0 }, &info()).unwrap();
        assert!(r.uniforms.is_empty());
        assert_eq!(r.uniform_binding, None);
        assert!(r.context.globals.is_empty());
        assert_eq!(r.textures.len(), 1);
        assert_eq!(r.textures[0].file, "a.png");
        assert!(r.approximations.is_empty());
    }

    #[test]
    fn without_uniforms_textures_start_at_first_binding() {
        let r = reflect(&sample_dag(false), &BindingOptions { group: 0, first_binding: 4 }, &info()).unwrap();
        assert_eq!(r.uniform_binding, None);
        assert_eq!(r.uniform_block_size, 0);
        assert_eq!(r.textures[0].binding, 4);
        assert_eq!(r.textures[0].sampler_binding, 5);
    }

    #[test]
    fn geomprop_functions_are_unique_and_clean() {
        let mut dag = Dag::new("m");
        for name in ["my-prop", "my_prop", "plain"] {
            let id = dag.add(node(
                NodeKind::Context(ContextRead::GeomProp { name: name.into(), ty: Type::Float }),
                Type::Float,
            ));
            dag.surface.fields.push(SurfaceField { name: name.into(), ty: Type::Float, node: id });
        }
        let c = reflect(&dag, &BindingOptions { group: 0, first_binding: 0 }, &info()).unwrap().context;
        let f: Vec<&str> = c.geomprops.iter().map(|g| g.function.as_str()).collect();
        assert_eq!(
            f,
            vec![
                "shadewire_host_geomprop_my_prop",
                "shadewire_host_geomprop_my_prop_2",
                "shadewire_host_geomprop_plain"
            ]
        );
    }

    #[test]
    fn geomprop_default_is_reflected_and_serialised() {
        let mut dag = Dag::new("m");
        let g = dag.add(node(
            NodeKind::Context(ContextRead::GeomProp { name: "tint".into(), ty: Type::Color3 }),
            Type::Color3,
        ));
        let h = dag
            .add(node(NodeKind::Context(ContextRead::GeomProp { name: "bare".into(), ty: Type::Float }), Type::Float));
        dag.surface.fields.push(SurfaceField { name: "a".into(), ty: Type::Color3, node: g });
        dag.surface.fields.push(SurfaceField { name: "b".into(), ty: Type::Float, node: h });
        dag.geomprop_defaults.insert("tint".into(), Value::Color3([1.0, 1.0, 1.0]));
        let c = reflect(&dag, &BindingOptions { group: 0, first_binding: 0 }, &info()).unwrap().context;
        let bare = c.geomprops.iter().find(|g| g.name == "bare").unwrap();
        let tint = c.geomprops.iter().find(|g| g.name == "tint").unwrap();
        assert_eq!(bare.default, None);
        assert_eq!(tint.default, Some(Value::Color3([1.0, 1.0, 1.0])));
        let json = serde_json::to_string(tint).unwrap();
        assert!(json.contains("\"default\":[1.0,1.0,1.0]"), "{json}");
    }

    #[test]
    fn context_splits_by_class_and_lists_transforms() {
        let mut dag = Dag::new("m");
        let p = dag.add(node(NodeKind::Context(ContextRead::Position(Space::World)), Type::Vector3));
        dag.add(node(NodeKind::Context(ContextRead::Position(Space::World)), Type::Vector3));
        dag.add(node(NodeKind::Context(ContextRead::ViewDirection(Space::World)), Type::Vector3));
        dag.add(node(NodeKind::Context(ContextRead::Time), Type::Float));
        dag.add(node(NodeKind::Context(ContextRead::GeomProp { name: "tint".into(), ty: Type::Color3 }), Type::Color3));
        dag.add(node(
            NodeKind::Op {
                op: Op::Transform { kind: TransformKind::Point, from: Space::World, to: Space::Object },
                args: vec![p],
            },
            Type::Vector3,
        ));
        // Every node feeds a surface field so all of them are live.
        for n in 0..dag.nodes.len() {
            let id = NodeId(n as u32);
            let ty = dag.node(id).ty;
            dag.surface.fields.push(SurfaceField { name: format!("f{n}"), ty, node: id });
        }
        let c = reflect(&dag, &BindingOptions { group: 0, first_binding: 0 }, &info()).unwrap().context;
        assert_eq!(c.varyings, vec![ContextEntry { name: "position_world".into(), ty: Type::Vector3 }]);
        assert_eq!(c.derived, vec![ContextEntry { name: "view_direction_world".into(), ty: Type::Vector3 }]);
        assert_eq!(c.globals, vec![ContextEntry { name: "time".into(), ty: Type::Float }]);
        assert_eq!(
            c.geomprops,
            vec![GeomPropReflection {
                name: "tint".into(),
                ty: Type::Color3,
                function: "shadewire_host_geomprop_tint".into(),
                default: None
            }]
        );
        assert_eq!(c.transforms, vec!["transform_point_world_to_object".to_string()]);
    }
}
