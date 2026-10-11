//! Colorspace conversion through the cmlib nodedefs
//! `ND_<from>_to_<working>_<type>`, inlined like any implementation graph.

use std::collections::HashMap;

use shadewire_ir::{Error, Location, NodeId, Type};

use super::{Binding, Elaborator};

/// True when `name` asks for no conversion.
fn is_no_colorspace(name: &str) -> bool {
    name.is_empty() || name == "none"
}

impl<'a> Elaborator<'a> {
    /// Converts a `color3`/`color4` value from colorspace `from` to the working
    /// colorspace. No colorspace, `none`, the working colorspace itself and
    /// other types leave the value unchanged.
    pub fn convert_colorspace(&mut self, value: NodeId, ty: Type, from: Option<&str>, at: &Location) -> Result<NodeId, Error> {
        let from = match from {
            Some(name) if !is_no_colorspace(name) && name != self.options.working_colorspace => name,
            _ => return Ok(value),
        };
        if !matches!(ty, Type::Color3 | Type::Color4) {
            return Ok(value);
        }
        let name = format!("ND_{from}_to_{}_{}", self.options.working_colorspace, ty.mtlx_name());
        let unsupported = || Error::UnsupportedColorspace { at: at.clone(), colorspace: from.to_string() };
        let nd = self.index.nodedef(&name).ok_or_else(unsupported)?;
        let graph = self.index.graph_for_nodedef(&name).ok_or_else(unsupported)?;
        let mut interface = HashMap::new();
        interface.insert("in".to_string(), Binding::Node(value));
        let path = format!("{}/{name}", at.path);
        self.inline_bound(nd, graph, interface, path, &name, None, at)
    }
}
