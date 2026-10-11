use crate::{ContextRead, Op, Type, Value};

/// Index of a node in `Dag::nodes`. Nodes are stored in dependency order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

impl NodeId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    /// A value baked into the shader.
    Const(Value),
    /// A value the host may change without recompiling.
    Uniform {
        name: String,
        value: Value,
    },
    /// A value the host supplies per fragment, per frame or per geometry.
    Context(ContextRead),
    /// A texture lookup; `texture` indexes `Dag::textures`. `grad` is the screen-space
    /// derivative of `uv` along x and y, in MaterialX uv space, for an explicit-gradient
    /// lookup (`textureSampleGrad`); `None` lets the GPU take it from `uv` itself.
    Sample {
        texture: usize,
        uv: NodeId,
        default: Value,
        grad: Option<[NodeId; 2]>,
    },
    Op {
        op: Op,
        args: Vec<NodeId>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    pub ty: Type,
    /// MaterialX path such as `NG_marble/noise1`, for dumps and errors.
    pub origin: String,
}
