use crate::{Dag, Node, NodeId, NodeKind, SurfaceField};

/// Copies nodes from an old DAG into a new one, remapping arguments.
/// `copy` and `alias` are called once per old node, in order, so `map[i]` is old node `i`.
pub struct Rebuilder {
    pub new: Dag,
    pub map: Vec<NodeId>,
}

impl Rebuilder {
    pub fn new(old: &Dag) -> Rebuilder {
        let mut new = Dag::new(&old.surface.model);
        new.textures = old.textures.clone();
        new.approximations = old.approximations.clone();
        new.geomprop_defaults = old.geomprop_defaults.clone();
        Rebuilder { new, map: Vec::with_capacity(old.nodes.len()) }
    }

    pub fn remap(&self, old: NodeId) -> NodeId {
        self.map[old.index()]
    }

    pub fn remapped_kind(&self, kind: &NodeKind) -> NodeKind {
        match kind {
            NodeKind::Op { op, args } => NodeKind::Op { op: *op, args: args.iter().map(|a| self.remap(*a)).collect() },
            NodeKind::Sample { texture, uv, default } => {
                NodeKind::Sample { texture: *texture, uv: self.remap(*uv), default: default.clone() }
            }
            other => other.clone(),
        }
    }

    /// Adds `old`'s copy and records the mapping for the next old node.
    pub fn copy(&mut self, old: &Node) -> NodeId {
        let id = self.new.add(Node { kind: self.remapped_kind(&old.kind), ty: old.ty, origin: old.origin.clone() });
        self.map.push(id);
        id
    }

    /// Records that the next old node is replaced by an existing new node.
    pub fn alias(&mut self, to_new: NodeId) {
        self.map.push(to_new);
    }

    pub fn finish(mut self, old: &Dag) -> Dag {
        self.new.surface.fields = old
            .surface
            .fields
            .iter()
            .map(|f| SurfaceField { name: f.name.clone(), ty: f.ty, node: self.remap(f.node) })
            .collect();
        self.new
    }
}
