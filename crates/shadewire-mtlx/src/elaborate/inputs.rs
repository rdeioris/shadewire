//! Input resolution: connections, nodegraph references, interface bindings,
//! authored values, then nodedef defaults.

use shadewire_ir::{Error, Location, Node as IrNode, NodeId, NodeKind};

use super::{Binding, Elaborator, Scope, TextValue};
use crate::document::{Input, Node, NodeDef, PortDef};

impl<'a> Elaborator<'a> {
    /// The value of value-typed input `input_name` of `node`, whose nodedef is `nd`.
    pub fn input_value(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, input_name: &str) -> Result<NodeId, Error> {
        self.input_node(scope, node, nd, input_name, false, false)
    }

    /// `input_value` for an input of a surface node: a value authored on the
    /// node becomes a uniform when `uniform_eligible` (the node is in the user's scope).
    pub fn surface_input_value(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        nd: &'a NodeDef,
        input_name: &str,
        uniform_eligible: bool,
    ) -> Result<NodeId, Error> {
        self.input_node(scope, node, nd, input_name, false, uniform_eligible)
    }

    /// `input_value` for an op parameter or compile-time setting: a uniform
    /// bound through an interface is read as a constant of its value instead.
    pub fn param_value(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, input_name: &str) -> Result<NodeId, Error> {
        self.input_node(scope, node, nd, input_name, true, false)
    }

    fn input_node(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        nd: &'a NodeDef,
        input_name: &str,
        param: bool,
        uniform_eligible: bool,
    ) -> Result<NodeId, Error> {
        let given = node.inputs.iter().find(|i| i.name == input_name);
        let path = format!("{}.{input_name}", scope.origin(&node.name));
        let at = self.at(given.map_or(node.line, |i| i.line), path);
        let port = nd.inputs.iter().find(|p| p.name == input_name).ok_or_else(|| Error::BadValue {
            at: at.clone(),
            message: format!("{} has no input {input_name}", nd.name),
        })?;
        let ty = self.value_type(&port.port_type, &at)?;
        let raw = match given {
            Some(input) => self.given_input(scope, input, ty, param, uniform_eligible, &at)?,
            None => None,
        };
        let id = match raw {
            Some(id) => id,
            None => self.nodedef_default(port, ty, &at)?,
        };
        self.convert_input(id, given, port, &at)
    }

    /// Steps 1-4: what the node itself says. `None` means use the nodedef default.
    fn given_input(
        &mut self,
        scope: &Scope<'a>,
        input: &Input,
        ty: shadewire_ir::Type,
        param: bool,
        uniform_eligible: bool,
        at: &Location,
    ) -> Result<Option<NodeId>, Error> {
        if let Some(nodename) = &input.nodename {
            return self.node_output(scope, nodename, input.output.as_deref()).map(Some);
        }
        if let Some(graph) = &input.nodegraph {
            let inner = self.user_graph_scope(graph, at)?;
            return self.graph_output(&inner, input.output.as_deref(), at).map(Some);
        }
        if let Some(name) = &input.interfacename {
            match scope.interface.get(name) {
                Some(Binding::Node(id)) => return Ok(Some(self.as_param(*id, param))),
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
            return self.authored_value(scope, &input.name, ty, text, uniform_eligible, at).map(Some);
        }
        Ok(None)
    }

    /// A string or filename input: the node's value, its interface binding, or the nodedef value.
    pub fn string_input(&mut self, scope: &Scope<'a>, node: &Node, nd: &NodeDef, input_name: &str) -> Result<Option<String>, Error> {
        Ok(self.text_input(scope, node, nd, input_name)?.map(|t| t.text))
    }

    /// `string_input` with the value's colorspace. Through an interface the
    /// binding's colorspace wins, falling back to the node input's own.
    pub fn text_input(&mut self, scope: &Scope<'a>, node: &Node, nd: &NodeDef, input_name: &str) -> Result<Option<TextValue>, Error> {
        let given = node.inputs.iter().find(|i| i.name == input_name);
        if let Some(input) = given {
            if let Some(name) = &input.interfacename {
                match scope.interface.get(name) {
                    Some(Binding::Text(bound)) => {
                        let colorspace = bound.colorspace.clone().or_else(|| input.colorspace.clone());
                        return Ok(Some(TextValue { text: bound.text.clone(), colorspace }));
                    }
                    Some(Binding::Node(_)) => {
                        let at = self.at(input.line, format!("{}.{input_name}", scope.origin(&node.name)));
                        return Err(Error::BadValue { at, message: format!("interface input {name} is not a string") });
                    }
                    Some(Binding::Unbound) | None => {}
                }
            } else if let Some(text) = &input.value {
                return Ok(Some(TextValue { text: text.clone(), colorspace: input.colorspace.clone() }));
            }
        }
        let port = nd.inputs.iter().find(|p| p.name == input_name);
        Ok(port.and_then(|p| p.value.clone().map(|text| TextValue { text, colorspace: p.colorspace.clone() })))
    }

    /// A uniform read as a parameter becomes a constant of its value.
    fn as_param(&mut self, id: NodeId, param: bool) -> NodeId {
        let node = self.dag.node(id);
        match &node.kind {
            NodeKind::Uniform { value, .. } if param => {
                let constant = IrNode { kind: NodeKind::Const(value.clone()), ty: node.ty, origin: node.origin.clone() };
                self.dag.add(constant)
            }
            _ => id,
        }
    }

    /// Colorspace then unit conversion of a value authored on the node itself.
    /// Connections and interface bindings arrive already converted.
    pub fn convert_input(&mut self, id: NodeId, given: Option<&Input>, port: &PortDef, at: &Location) -> Result<NodeId, Error> {
        let Some(input) = given else { return Ok(id) };
        let authored = input.value.is_some() && input.nodename.is_none() && input.nodegraph.is_none() && input.interfacename.is_none();
        if !authored {
            return Ok(id);
        }
        let ty = self.dag.node(id).ty;
        let id = self.convert_colorspace(id, ty, input.colorspace.as_deref(), at)?;
        let unittype = input.unittype.as_deref().or(port.unittype.as_deref());
        self.convert_unit(id, ty, input.unit.as_deref(), unittype, port.unit.as_deref(), at)
    }
}
