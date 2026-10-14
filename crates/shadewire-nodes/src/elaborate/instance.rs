//! One node instance: memo and cycle check, nodedef resolution, then inline,
//! special or leaf.

use std::collections::HashMap;

use shadewire::{Error, Location, Node as IrNode, NodeId, NodeKind, Value, op_for_category};

use super::ramp::is_ramp;
use super::special::is_special;
use super::{Binding, Elaborator, MAX_GRAPH_DEPTH, MemoKey, Scope};
use crate::document::{Node, NodeDef, NodeGraph, Output, PortType};
use crate::resolve_nodedef::resolve_nodedef;

impl<'a> Elaborator<'a> {
    /// Output `output` (or the only one) of the node named `node_name` in `scope`.
    pub fn node_output(&mut self, scope: &Scope<'a>, node_name: &str, output: Option<&str>) -> Result<NodeId, Error> {
        let origin = scope.origin(node_name);
        let node = scope.find_node(node_name).ok_or_else(|| Error::BadValue {
            at: self.at(scope, scope.graph.map_or(0, |g| g.line), origin.clone()),
            message: format!("no node named {node_name}"),
        })?;
        // A single-output node has one output whatever name it is read by.
        let output_key = match node.port_type {
            PortType::MultiOutput => output.unwrap_or(""),
            _ => "",
        };
        let key: MemoKey = (scope.id, node_name.to_string(), output_key.to_string());
        if let Some(id) = self.memo.get(&key) {
            return Ok(*id);
        }
        let at = self.at(scope, node.line, origin);
        if self.depth >= MAX_GRAPH_DEPTH {
            return Err(Error::BadValue { at, message: format!("graph deeper than {MAX_GRAPH_DEPTH} nodes") });
        }
        if !self.in_progress.insert(key.clone()) {
            return Err(Error::Cycle { at, node: node_name.to_string() });
        }
        self.depth += 1;
        let result = self.instance(scope, node, output, &at);
        self.depth -= 1;
        self.in_progress.remove(&key);
        let id = result?;
        self.check_node_limit()?;
        self.memo.insert(key, id);
        Ok(id)
    }

    fn instance(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        output: Option<&str>,
        at: &Location,
    ) -> Result<NodeId, Error> {
        if let PortType::Value(ty) = &node.port_type {
            self.value_type(&PortType::Value(*ty), at)?;
        }
        if node.nodedef.is_none() && self.index.nodedefs_for_category(&node.category).is_empty() {
            return Err(Error::UnknownNode { at: at.clone(), category: node.category.clone() });
        }
        let input_types: Vec<(String, PortType)> =
            node.inputs.iter().map(|i| (i.name.clone(), i.port_type.clone())).collect();
        let nd = resolve_nodedef(&self.index, node, &input_types, at)?;
        if let Some(graph) = self.index.graph_for_nodedef(&nd.name) {
            return self.inline(scope, node, nd, graph, output, at);
        }
        if is_special(&node.category) {
            return self.elaborate_special(scope, node, nd, output);
        }
        let out = self.output_type(nd, output, at)?;
        if is_ramp(&node.category) {
            return self.elaborate_ramp(scope, node, nd, out, at);
        }
        let value_inputs = nd.inputs.iter().filter(|p| matches!(p.port_type, PortType::Value(_))).count();
        match op_for_category(&node.category, out, value_inputs) {
            Some(op) => self.leaf(scope, node, nd, op, out, at),
            None => Err(Error::MissingLeafImpl { at: at.clone(), nodedef: nd.name.clone() }),
        }
    }

    /// The value type of nodedef output `output` (or its first output).
    pub fn output_type(&self, nd: &NodeDef, output: Option<&str>, at: &Location) -> Result<shadewire::Type, Error> {
        let port = match output {
            Some(name) => nd.outputs.iter().find(|o| o.name == name),
            None => nd.outputs.first(),
        };
        let port = port.ok_or_else(|| Error::BadValue {
            at: at.clone(),
            message: format!("{} has no output {}", nd.name, output.unwrap_or("")),
        })?;
        self.value_type(&port.port_type, at)
    }

    /// Instantiates an implementation graph: every nodedef input is bound in
    /// the caller's scope first, then the requested graph output is followed
    /// inside. Only that body holds the recursion guard, so chained instances
    /// of one nodedef are fine while a graph that reaches its own nodedef is a cycle.
    fn inline(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        nd: &'a NodeDef,
        graph: &'a NodeGraph,
        output: Option<&str>,
        at: &Location,
    ) -> Result<NodeId, Error> {
        let interface = self.bind_interface(scope, node, nd, false)?;
        self.inline_bound(nd, graph, interface, scope.origin(&node.name), &node.name, output, at)
    }

    /// Binds every nodedef input of `node`, read in the caller's scope, as
    /// the interface of its implementation graph. Authored values become
    /// uniforms when `uniform_eligible`.
    pub fn bind_interface(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        nd: &'a NodeDef,
        uniform_eligible: bool,
    ) -> Result<HashMap<String, Binding>, Error> {
        let mut interface = HashMap::new();
        for port in &nd.inputs {
            let binding = match &port.port_type {
                PortType::String | PortType::Filename => match self.text_input(scope, node, nd, &port.name)? {
                    Some(text) => Binding::Text(text),
                    None => Binding::Unbound,
                },
                PortType::Value(_) => {
                    Binding::Node(self.surface_input_value(scope, node, nd, &port.name, uniform_eligible)?)
                }
                _ => Binding::Unbound,
            };
            interface.insert(port.name.clone(), binding);
        }
        Ok(interface)
    }

    /// Follows output `output` (or the nodedef's first) of implementation
    /// graph `graph` with its interface already bound. `path` names the instance.
    #[allow(clippy::too_many_arguments)]
    pub fn inline_bound(
        &mut self,
        nd: &'a NodeDef,
        graph: &'a NodeGraph,
        interface: HashMap<String, Binding>,
        path: String,
        node_name: &str,
        output: Option<&str>,
        at: &Location,
    ) -> Result<NodeId, Error> {
        let file = self.index.graph_file(graph);
        let child = Scope {
            id: self.new_scope_id(),
            graph: Some(graph),
            nodes: &graph.nodes,
            interface,
            user: false,
            path,
            file,
        };
        let wanted = output.or(nd.outputs.first().map(|o| o.name.as_str()));
        if !self.inlining.insert(nd.name.clone()) {
            return Err(Error::Cycle { at: at.clone(), node: node_name.to_string() });
        }
        let result = self.graph_output(&child, wanted, at);
        self.inlining.remove(&nd.name);
        result
    }

    /// The graph output named `output` of the scope's graph, or its only output.
    pub fn graph_output(&mut self, scope: &Scope<'a>, output: Option<&str>, at: &Location) -> Result<NodeId, Error> {
        let graph =
            scope.graph.ok_or_else(|| Error::BadValue { at: at.clone(), message: "no graph in scope".to_string() })?;
        let named = output.and_then(|name| graph.outputs.iter().find(|o| o.name == name));
        let found = match (named, graph.outputs.as_slice()) {
            (Some(o), _) => o,
            (None, [only]) => only,
            _ => {
                return Err(Error::BadValue {
                    at: at.clone(),
                    message: format!("{} has no output {}", graph.name, output.unwrap_or("")),
                });
            }
        };
        let key = (scope.id, found.name.clone());
        if !self.following_outputs.insert(key.clone()) {
            let at = self.at(scope, found.line, scope.origin(&found.name));
            return Err(Error::Cycle { at, node: format!("{}/{}", graph.name, found.name) });
        }
        let result = self.follow_output(scope, found);
        self.following_outputs.remove(&key);
        result
    }

    /// Where a graph `<output>` element points: a node, a nodegraph or an interface input.
    fn follow_output(&mut self, scope: &Scope<'a>, output: &Output) -> Result<NodeId, Error> {
        let at = self.at(scope, output.line, scope.origin(&output.name));
        if let Some(nodename) = &output.nodename {
            return self.node_output(scope, nodename, output.output.as_deref());
        }
        if let Some(graph) = &output.nodegraph {
            let inner = self.user_graph_scope(graph, &at)?;
            return self.graph_output(&inner, output.output.as_deref(), &at);
        }
        if let Some(name) = &output.interfacename {
            return match scope.interface.get(name) {
                Some(Binding::Node(id)) => Ok(*id),
                _ => {
                    let ty = self.value_type(&output.port_type, &at)?;
                    Ok(self.dag.add(IrNode { kind: NodeKind::Const(Value::zero(ty)), ty, origin: at.path.clone() }))
                }
            };
        }
        Err(Error::BadValue { at, message: format!("output {} is not connected", output.name) })
    }
}
