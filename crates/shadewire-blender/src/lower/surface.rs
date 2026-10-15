//! Shader sockets: what the Material Output's Surface reads, and the dag finished from it.

use shadewire::{Dag, Error, NodeId};
use shadewire_nodes::{Arg, ShaderNode};

use super::{Lowerer, principled};
use crate::document::at;
use crate::graph::FlatNode;

/// What a shader socket carries.
pub enum Shading {
    /// A Principled BSDF: `open_pbr_surface` over these fields.
    Principled { fields: Vec<(String, Arg)>, origin: String },
    /// A tree of lobes and operators.
    Closure(Tree),
    /// Nothing is linked.
    Nothing,
}

/// A closure tree: its shader-level nodes, and the names of its BSDF and EDF roots, with the transparency mixed
/// over them.
pub struct Tree {
    pub nodes: Vec<ShaderNode>,
    pub bsdf: Option<String>,
    pub edf: Option<String>,
    pub transparency: Option<NodeId>,
}

/// The shading `node`'s shader output carries (the output's name matters to no shader this version lowers).
pub fn shading(l: &mut Lowerer, node: &FlatNode, _output: &str) -> Result<Shading, Error> {
    match node.kind.as_str() {
        "ShaderNodeBsdfPrincipled" => principled::fields(l, node),
        kind => Err(Error::UnsupportedSurface {
            at: at(&node.name),
            node: node.name.clone(),
            reason: format!("{kind} is not a shader this version lowers"),
        }),
    }
}

/// The dag finished with `shading` as the surface of `output_node`, and the lowering's warnings.
pub fn finish(l: Lowerer, shading: Shading, output_node: &FlatNode) -> Result<(Dag, Vec<String>), Error> {
    let unsupported = |reason: &str| Error::UnsupportedSurface {
        at: at(&output_node.name),
        node: output_node.name.clone(),
        reason: reason.to_string(),
    };
    match shading {
        Shading::Principled { fields, origin } => {
            let fields: Vec<(&str, Arg)> = fields.iter().map(|(name, arg)| (name.as_str(), arg.clone())).collect();
            let Lowerer { e, warnings, .. } = l;
            Ok((e.finish_model("open_pbr_surface", &fields, &origin)?, warnings))
        }
        Shading::Closure(_) => Err(unsupported("closure trees are not lowered by this version")),
        Shading::Nothing => Err(unsupported("Surface is not linked")),
    }
}
