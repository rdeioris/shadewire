//! Input resolution: connections, nodegraph references, interface bindings,
//! authored values, then nodedef defaults.

use shadewire_ir::{Error, Location, NodeId};

use super::{Binding, Elaborator, Scope};
use crate::document::{Input, Node, NodeDef, PortDef};

impl<'a> Elaborator<'a> {
    /// The value of value-typed input `input_name` of `node`, whose nodedef is `nd`.
    pub fn input_value(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, input_name: &str) -> Result<NodeId, Error> {
        let given = node.inputs.iter().find(|i| i.name == input_name);
        let path = format!("{}.{input_name}", scope.origin(&node.name));
        let at = self.at(given.map_or(node.line, |i| i.line), path);
        let port = nd.inputs.iter().find(|p| p.name == input_name).ok_or_else(|| Error::BadValue {
            at: at.clone(),
            message: format!("{} has no input {input_name}", nd.name),
        })?;
        let ty = self.value_type(&port.port_type, &at)?;
        let raw = match given {
            Some(input) => self.given_input(scope, input, ty, &at)?,
            None => None,
        };
        let id = match raw {
            Some(id) => id,
            None => self.nodedef_default(port, ty, &at)?,
        };
        self.convert_input(id, given, port, &at)
    }

    /// Steps 1-4: what the node itself says. `None` means use the nodedef default.
    fn given_input(&mut self, scope: &Scope<'a>, input: &Input, ty: shadewire_ir::Type, at: &Location) -> Result<Option<NodeId>, Error> {
        if let Some(nodename) = &input.nodename {
            return self.node_output(scope, nodename, input.output.as_deref()).map(Some);
        }
        if let Some(graph) = &input.nodegraph {
            let inner = self.user_graph_scope(graph, at)?;
            return self.graph_output(&inner, input.output.as_deref(), at).map(Some);
        }
        if let Some(name) = &input.interfacename {
            match scope.interface.get(name) {
                Some(Binding::Node(id)) => return Ok(Some(*id)),
                Some(Binding::Text(_)) => {
                    return Err(Error::BadValue {
                        at: at.clone(),
                        message: format!("interface input {name} is a string, not a {}", ty.mtlx_name()),
                    })
                }
                Some(Binding::Unbound) | None => return Ok(None),
            }
        }
        if let Some(text) = &input.value {
            return self.authored_value(scope, &input.name, ty, text, false, at).map(Some);
        }
        Ok(None)
    }

    /// A string or filename input: the node's value, its interface binding, or the nodedef value.
    pub fn string_input(&mut self, scope: &Scope<'a>, node: &Node, nd: &NodeDef, input_name: &str) -> Result<Option<String>, Error> {
        let given = node.inputs.iter().find(|i| i.name == input_name);
        if let Some(input) = given {
            if let Some(name) = &input.interfacename {
                match scope.interface.get(name) {
                    Some(Binding::Text(text)) => return Ok(Some(text.clone())),
                    Some(Binding::Node(_)) => {
                        let at = self.at(input.line, format!("{}.{input_name}", scope.origin(&node.name)));
                        return Err(Error::BadValue { at, message: format!("interface input {name} is not a string") });
                    }
                    Some(Binding::Unbound) | None => {}
                }
            } else if let Some(text) = &input.value {
                return Ok(Some(text.clone()));
            }
        }
        Ok(nd.inputs.iter().find(|p| p.name == input_name).and_then(|p| p.value.clone()))
    }

    /// Units and colorspace conversion of an input (Task 17). The identity for now.
    pub fn convert_input(&mut self, id: NodeId, _given: Option<&Input>, _port: &PortDef, _at: &Location) -> Result<NodeId, Error> {
        Ok(id)
    }
}
