use serde::Serialize;

use crate::{ContextClass, ContextRead, Dag, NodeId, NodeKind, Op, transform_name};

/// What a surface field's value depends on in the graph as compiled: the inputs its node reaches.
/// Every list is sorted and empty when nothing of that kind is reached; a field with all lists
/// empty is constant. The classes follow `ContextReflection`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct FieldDepends {
    /// Uniform names, as in `uniforms`.
    pub uniforms: Vec<String>,
    /// `Dag::textures` indices of the textures sampled, as `TextureReflection::index`.
    pub textures: Vec<usize>,
    /// Per-vertex reads by reflection name: `position_world`, `normal_object`, `texcoord_0`, `geomcolor_0`...
    pub varyings: Vec<String>,
    /// Reads the host computes from others: `view_direction_world`...
    pub derived: Vec<String>,
    /// Per-frame reads: `time`, `frame`.
    pub globals: Vec<String>,
    /// Geometry properties by authored name.
    pub geomprops: Vec<String>,
    /// Space transforms the host supplies, as in `ContextReflection::transforms`.
    pub transforms: Vec<String>,
}

impl FieldDepends {
    /// True when the field reads nothing the host provides: no uniform, texture, context read or transform.
    pub fn is_constant(&self) -> bool {
        *self == FieldDepends::default()
    }
}

fn push(list: &mut Vec<String>, name: String) {
    if !list.contains(&name) {
        list.push(name);
    }
}

/// The inputs `root` reaches.
pub fn depends(dag: &Dag, root: NodeId) -> FieldDepends {
    let mut out = FieldDepends::default();
    let mut seen = vec![false; dag.nodes.len()];
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if std::mem::replace(&mut seen[id.index()], true) {
            continue;
        }
        match &dag.node(id).kind {
            NodeKind::Const(_) => {}
            NodeKind::Uniform { name, .. } => push(&mut out.uniforms, name.clone()),
            NodeKind::Context(read) => match (read.class(), read) {
                (ContextClass::GeomProp, ContextRead::GeomProp { name, .. }) => push(&mut out.geomprops, name.clone()),
                (ContextClass::Varying, _) => push(&mut out.varyings, read.reflection_name()),
                (ContextClass::Derived, _) => push(&mut out.derived, read.reflection_name()),
                (ContextClass::Global, _) => push(&mut out.globals, read.reflection_name()),
                (ContextClass::GeomProp, _) => unreachable!("only GeomProp reads are of the GeomProp class"),
            },
            NodeKind::Sample { texture, .. } => {
                if !out.textures.contains(texture) {
                    out.textures.push(*texture);
                }
            }
            NodeKind::Op { op: Op::Transform { kind, from, to }, .. } => {
                push(&mut out.transforms, transform_name(*kind, *from, *to))
            }
            NodeKind::Op { .. } => {}
        }
        stack.extend(dag.args(id));
    }
    out.uniforms.sort();
    out.textures.sort();
    out.varyings.sort();
    out.derived.sort();
    out.globals.sort();
    out.geomprops.sort();
    out.transforms.sort();
    out
}

#[cfg(test)]
mod tests {
    use crate::reflection::{BindingOptions, Reflection, SourceInfo, reflect};
    use crate::*;

    fn add(dag: &mut Dag, kind: NodeKind, ty: Type) -> NodeId {
        dag.add(Node { kind, ty, origin: String::new() })
    }

    fn field(dag: &mut Dag, name: &str, node: NodeId) {
        let ty = dag.node(node).ty;
        dag.surface.fields.push(SurfaceField { name: name.into(), ty, node });
    }

    fn reflected(dag: &Dag) -> Reflection {
        let info = SourceInfo { source: "a.mtlx".into(), mtlx_version: "1.39".into() };
        reflect(dag, &BindingOptions { group: 0, first_binding: 0, reserve_uniform_binding: false }, &info).unwrap()
    }

    fn color(dag: &mut Dag, v: f32) -> NodeId {
        add(dag, NodeKind::Const(Value::Color3([v; 3])), Type::Color3)
    }

    #[test]
    fn a_constant_material_depends_on_nothing() {
        let mut dag = Dag::new("open_pbr_surface");
        let c = color(&mut dag, 0.8);
        let w = add(&mut dag, NodeKind::Const(Value::Float(1.0)), Type::Float);
        field(&mut dag, "base_color", c);
        field(&mut dag, "base_weight", w);
        let r = reflected(&dag);
        assert!(r.params["base_color"].depends.is_constant());
        assert!(r.params["base_weight"].depends.is_constant());
        let json = serde_json::to_string(&r.params["base_color"].depends).unwrap();
        assert_eq!(
            json,
            r#"{"uniforms":[],"textures":[],"varyings":[],"derived":[],"globals":[],"geomprops":[],"transforms":[]}"#
        );
    }

    #[test]
    fn a_base_color_over_a_parameter_names_it() {
        let mut dag = Dag::new("open_pbr_surface");
        let tint =
            add(&mut dag, NodeKind::Uniform { name: "tint".into(), value: Value::Color3([1.0; 3]) }, Type::Color3);
        let gain = add(&mut dag, NodeKind::Uniform { name: "gain".into(), value: Value::Float(1.0) }, Type::Float);
        let c = color(&mut dag, 0.5);
        let a = add(&mut dag, NodeKind::Op { op: Op::Multiply, args: vec![tint, c] }, Type::Color3);
        let b = add(&mut dag, NodeKind::Op { op: Op::Multiply, args: vec![a, gain] }, Type::Color3);
        field(&mut dag, "base_color", b);
        let rough = add(&mut dag, NodeKind::Uniform { name: "rough".into(), value: Value::Float(0.5) }, Type::Float);
        field(&mut dag, "specular_roughness", rough);
        let r = reflected(&dag);
        let d = &r.params["base_color"].depends;
        assert_eq!(d.uniforms, vec!["gain".to_string(), "tint".to_string()]);
        assert!(d.textures.is_empty() && d.varyings.is_empty() && d.globals.is_empty());
        assert_eq!(r.params["specular_roughness"].depends.uniforms, vec!["rough".to_string()]);
    }

    #[test]
    fn a_texture_on_roughness_only_is_not_on_base_color() {
        let mut dag = Dag::new("open_pbr_surface");
        let _unused = dag.add_texture(TextureDesc {
            file: "dead.png".into(),
            colorspace: None,
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Periodic,
            filter: FilterMode::Linear,
        });
        let tex = dag.add_texture(TextureDesc {
            file: "rough.png".into(),
            colorspace: None,
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Periodic,
            filter: FilterMode::Linear,
        });
        let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
        let s =
            add(&mut dag, NodeKind::Sample { texture: tex, uv, default: Value::Float(0.5), grad: None }, Type::Float);
        let c = color(&mut dag, 0.8);
        field(&mut dag, "base_color", c);
        field(&mut dag, "specular_roughness", s);
        let r = reflected(&dag);
        assert!(r.params["base_color"].depends.is_constant());
        let d = &r.params["specular_roughness"].depends;
        assert_eq!(d.textures, vec![tex]);
        assert_eq!(r.textures[0].index, tex);
        assert_eq!(d.varyings, vec!["texcoord_0".to_string()]);
    }

    #[test]
    fn time_driven_emission_lists_the_global() {
        let mut dag = Dag::new("open_pbr_surface");
        let t = add(&mut dag, NodeKind::Context(ContextRead::Time), Type::Float);
        let s = add(&mut dag, NodeKind::Op { op: Op::Sin, args: vec![t] }, Type::Float);
        let lum = add(&mut dag, NodeKind::Uniform { name: "lum".into(), value: Value::Float(10.0) }, Type::Float);
        let e = add(&mut dag, NodeKind::Op { op: Op::Multiply, args: vec![s, lum] }, Type::Float);
        let c = color(&mut dag, 1.0);
        field(&mut dag, "emission_luminance", e);
        field(&mut dag, "emission_color", c);
        let r = reflected(&dag);
        let d = &r.params["emission_luminance"].depends;
        assert_eq!(d.globals, vec!["time".to_string()]);
        assert_eq!(d.uniforms, vec!["lum".to_string()]);
        assert!(r.params["emission_color"].depends.is_constant());
    }

    #[test]
    fn a_base_color_over_position_lists_the_varying_and_transform() {
        let mut dag = Dag::new("open_pbr_surface");
        let p = add(&mut dag, NodeKind::Context(ContextRead::Position(Space::World)), Type::Vector3);
        let o = add(
            &mut dag,
            NodeKind::Op {
                op: Op::Transform { kind: TransformKind::Point, from: Space::World, to: Space::Object },
                args: vec![p],
            },
            Type::Vector3,
        );
        let v = add(&mut dag, NodeKind::Context(ContextRead::ViewDirection(Space::World)), Type::Vector3);
        let g = add(
            &mut dag,
            NodeKind::Context(ContextRead::GeomProp { name: "tint".into(), ty: Type::Vector3 }),
            Type::Vector3,
        );
        let a = add(&mut dag, NodeKind::Op { op: Op::Add, args: vec![o, v] }, Type::Vector3);
        let b = add(&mut dag, NodeKind::Op { op: Op::Add, args: vec![a, g] }, Type::Vector3);
        field(&mut dag, "base_color", b);
        let d = &reflected(&dag).params["base_color"].depends;
        assert_eq!(d.varyings, vec!["position_world".to_string()]);
        assert_eq!(d.transforms, vec!["transform_point_world_to_object".to_string()]);
        assert_eq!(d.derived, vec!["view_direction_world".to_string()]);
        assert_eq!(d.geomprops, vec!["tint".to_string()]);
        assert!(!d.is_constant());
    }
}
