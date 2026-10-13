use serde::Serialize;

use crate::{AddressMode, Dag, FilterMode, Node, NodeKind, Op};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextureReflection {
    pub index: usize,
    pub binding: u32,
    pub sampler_binding: u32,
    pub file: String,
    /// The colorspace the file was authored in, for information only: the
    /// generated shader already converts each sample to the working colorspace,
    /// so hosts must bind a raw (non-sRGB) texture view. An `*_srgb` view
    /// would decode the texels a second time.
    pub colorspace: Option<String>,
    pub address: [AddressMode; 2],
    pub filter: FilterMode,
}

/// Textures a live Sample reads, as `Dag::textures` indices in ascending order.
fn used(dag: &Dag, live: &[bool]) -> Vec<usize> {
    let mut used: Vec<usize> = Vec::new();
    for (i, node) in dag.nodes.iter().enumerate() {
        if let NodeKind::Sample { texture, .. } = &node.kind
            && live[i]
            && !used.contains(texture)
        {
            used.push(*texture);
        }
    }
    used.sort();
    used
}

/// Each used texture takes two bindings (texture, sampler) starting at `first`.
pub fn textures(dag: &Dag, live: &[bool], first: u32) -> Vec<TextureReflection> {
    let mut out = Vec::new();
    for (slot, index) in used(dag, live).into_iter().enumerate() {
        let t = &dag.textures[index];
        out.push(TextureReflection {
            index,
            binding: first + 2 * slot as u32,
            sampler_binding: first + 2 * slot as u32 + 1,
            file: t.file.clone(),
            colorspace: t.colorspace.clone(),
            address: [t.address_u, t.address_v],
            filter: t.filter,
        });
    }
    out
}

/// The note a DAG with screen-space derivatives (the gradients of hextile samples) carries:
/// `shadewire_eval_surface` then runs only in a fragment shader, not in compute.
pub const FRAGMENT_ONLY: &str = "fragment stage only (derivatives)";

/// Approximations and limits the used textures and their gradients force on the generated code.
pub fn approximations(dag: &Dag, live: &[bool]) -> Vec<String> {
    let mut out = Vec::new();
    if used(dag, live).iter().any(|i| dag.textures[*i].filter == FilterMode::Cubic) {
        out.push("cubic filter as linear".to_string());
    }
    let derivative = |n: &Node| matches!(n.kind, NodeKind::Op { op: Op::Dpdx | Op::Dpdy, .. });
    if dag.nodes.iter().zip(live).any(|(n, l)| *l && derivative(n)) {
        out.push(FRAGMENT_ONLY.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reflection::live::live_nodes;
    use crate::{SurfaceField, TextureDesc, Type, Value};

    fn desc(file: &str, filter: FilterMode) -> TextureDesc {
        TextureDesc {
            file: file.into(),
            colorspace: None,
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Clamp,
            filter,
        }
    }

    /// Adds a texture and a surface field that samples it.
    fn sampled(dag: &mut Dag, file: &str, filter: FilterMode) {
        let texture = dag.add_texture(desc(file, filter));
        let uv =
            dag.add(Node { kind: NodeKind::Const(Value::Vector2([0.0; 2])), ty: Type::Vector2, origin: String::new() });
        let s = dag.add(Node {
            kind: NodeKind::Sample { texture, uv, default: Value::Float(0.0), grad: None },
            ty: Type::Float,
            origin: String::new(),
        });
        dag.surface.fields.push(SurfaceField { name: file.into(), ty: Type::Float, node: s });
    }

    #[test]
    fn cubic_filter_is_reported_once() {
        let mut dag = Dag::new("m");
        sampled(&mut dag, "a.png", FilterMode::Cubic);
        sampled(&mut dag, "b.png", FilterMode::Cubic);
        assert_eq!(approximations(&dag, &live_nodes(&dag)), vec!["cubic filter as linear".to_string()]);
        let mut linear = Dag::new("m");
        sampled(&mut linear, "a.png", FilterMode::Linear);
        assert!(approximations(&linear, &live_nodes(&linear)).is_empty());
    }

    #[test]
    fn live_derivatives_are_fragment_only() {
        let mut dag = Dag::new("m");
        let uv = dag.add(Node {
            kind: NodeKind::Context(crate::ContextRead::TexCoord { index: 0, ty: Type::Vector2 }),
            ty: Type::Vector2,
            origin: String::new(),
        });
        let dead = dag.add(Node {
            kind: NodeKind::Op { op: Op::Dpdy, args: vec![uv] },
            ty: Type::Vector2,
            origin: String::new(),
        });
        dag.surface.fields.push(SurfaceField { name: "uv".into(), ty: Type::Vector2, node: uv });
        assert!(approximations(&dag, &live_nodes(&dag)).is_empty(), "{dead:?} is dead");
        let dx = dag.add(Node {
            kind: NodeKind::Op { op: Op::Dpdx, args: vec![uv] },
            ty: Type::Vector2,
            origin: String::new(),
        });
        dag.surface.fields.push(SurfaceField { name: "dx".into(), ty: Type::Vector2, node: dx });
        assert_eq!(approximations(&dag, &live_nodes(&dag)), vec![FRAGMENT_ONLY.to_string()]);
    }

    #[test]
    fn bindings_come_in_pairs() {
        let mut dag = Dag::new("m");
        sampled(&mut dag, "a.png", FilterMode::Linear);
        sampled(&mut dag, "b.png", FilterMode::Linear);
        let t = textures(&dag, &live_nodes(&dag), 3);
        assert_eq!((t[0].binding, t[0].sampler_binding), (3, 4));
        assert_eq!((t[1].binding, t[1].sampler_binding), (5, 6));
        assert_eq!(t[0].address, [AddressMode::Periodic, AddressMode::Clamp]);
    }
}
