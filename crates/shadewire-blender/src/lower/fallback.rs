//! Nodes this version does not lower: each output read is its default value, with a warning; a curve passes its
//! input through.

use shadewire::{Error, NodeId, Type, Value};

use super::Lowerer;
use super::values::{socket_type, socket_value};
use crate::document::at;
use crate::graph::FlatNode;

/// Output `socket` of the unsupported `node`: the value Blender shows on it (zero when none), as a constant.
pub fn default_output(l: &mut Lowerer, node: &FlatNode, socket: &str) -> Result<NodeId, Error> {
    let through = match node.kind.as_str() {
        "ShaderNodeRGBCurve" => Some(("Color", Type::Color3)),
        "ShaderNodeFloatCurve" => Some(("Value", Type::Float)),
        _ => None,
    };
    if let Some((input, ty)) = through {
        l.warn(node, "curve ignored, the input passes through");
        return l.input(node, input, ty);
    }
    let Some(out) = node.output(socket) else {
        return Err(Error::BadValue {
            at: at(&node.name),
            message: format!("{} \"{}\" has no output {socket}", node.kind, node.name),
        });
    };
    let Some(ty) = socket_type(&out.kind) else {
        return Err(Error::UnsupportedType { at: at(format!("{}.{socket}", node.name)), ty: out.kind.clone() });
    };
    l.warn(node, format!("not lowered, output {socket} is its default value"));
    let value = socket_value(out).unwrap_or_else(|| Value::zero(ty));
    Ok(l.e.constant(value, &format!("{}.{socket}", node.name)))
}
