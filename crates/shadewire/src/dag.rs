use std::collections::BTreeMap;

use crate::{Node, NodeId, NodeKind, Surface, TextureDesc, Value};

#[derive(Clone, Debug)]
pub struct Dag {
    pub nodes: Vec<Node>,
    pub textures: Vec<TextureDesc>,
    pub surface: Surface,
    pub approximations: Vec<String>,
    /// `geompropvalue` defaults by property name; reflection reports them to the host.
    pub geomprop_defaults: BTreeMap<String, Value>,
}

impl Dag {
    pub fn new(model: &str) -> Dag {
        Dag {
            nodes: Vec::new(),
            textures: Vec::new(),
            surface: Surface { model: model.to_string(), fields: Vec::new(), closure: None },
            approximations: Vec::new(),
            geomprop_defaults: BTreeMap::new(),
        }
    }

    /// Nodes are stored in dependency order: every argument already exists.
    pub fn add(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        for arg in node_args(&node.kind) {
            assert!(arg.0 < id.0, "forward reference from {:?} to {:?}", id, arg);
        }
        self.nodes.push(node);
        id
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub fn args(&self, id: NodeId) -> Vec<NodeId> {
        node_args(&self.node(id).kind)
    }

    /// Returns the index of an equal texture, adding it if none exists.
    pub fn add_texture(&mut self, texture: TextureDesc) -> usize {
        if let Some(index) = self.textures.iter().position(|t| *t == texture) {
            return index;
        }
        self.textures.push(texture);
        self.textures.len() - 1
    }
}

/// The nodes a kind reads: the uv and gradients of a sample, the arguments of an op.
pub fn node_args(kind: &NodeKind) -> Vec<NodeId> {
    match kind {
        NodeKind::Op { args, .. } => args.clone(),
        NodeKind::Sample { uv, grad: None, .. } => vec![*uv],
        NodeKind::Sample { uv, grad: Some([dx, dy]), .. } => vec![*uv, *dx, *dy],
        NodeKind::Const(_) | NodeKind::Uniform { .. } | NodeKind::Context(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    #[should_panic(expected = "forward reference")]
    fn args_must_exist_before_their_user() {
        let mut dag = Dag::new("open_pbr_surface");
        dag.add(Node {
            kind: NodeKind::Op { op: Op::Sin, args: vec![NodeId(5)] },
            ty: Type::Float,
            origin: String::new(),
        });
    }

    #[test]
    fn textures_are_deduplicated() {
        let mut dag = Dag::new("open_pbr_surface");
        let t = TextureDesc {
            file: "a.png".into(),
            colorspace: None,
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Periodic,
            filter: FilterMode::Linear,
        };
        assert_eq!(dag.add_texture(t.clone()), 0);
        assert_eq!(dag.add_texture(t), 0);
    }
}
