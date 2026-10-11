//! Scopes: the nodes a name can refer to and what their interface inputs are bound to.

use std::collections::HashMap;

use shadewire_ir::{Error, Location, NodeId};

use super::Elaborator;
use crate::document::{Node, NodeGraph, PortType};

/// What an interface input of a scope is bound to.
#[derive(Clone, Debug, PartialEq)]
pub enum Binding {
    /// A value input, already elaborated in the enclosing scope.
    Node(NodeId),
    /// A string or filename input, resolved in the enclosing scope.
    Text(String),
    /// Nothing given: readers fall back to their own nodedef default.
    Unbound,
}

/// A set of sibling nodes: the document's top level, a user nodegraph, or
/// one inlined instance of a library implementation graph.
#[derive(Clone, Debug)]
pub struct Scope<'a> {
    pub id: u32,
    pub graph: Option<&'a NodeGraph>,
    pub nodes: &'a [Node],
    pub interface: HashMap<String, Binding>,
    /// True for the top level and user-document nodegraphs, false inside library graphs.
    pub user: bool,
    /// Readable path of the scope: `""` at top level, the graph name for a
    /// user graph, `<parent path>/<node name>` for an inlined instance.
    pub path: String,
}

impl<'a> Scope<'a> {
    /// The origin of a node in this scope, such as `NG_marble/noise1`.
    pub fn origin(&self, node_name: &str) -> String {
        if self.path.is_empty() {
            node_name.to_string()
        } else {
            format!("{}/{node_name}", self.path)
        }
    }

    pub fn find_node(&self, name: &str) -> Option<&'a Node> {
        self.nodes.iter().find(|n| n.name == name)
    }
}

impl<'a> Elaborator<'a> {
    pub fn new_scope_id(&mut self) -> u32 {
        self.next_scope_id += 1;
        self.next_scope_id
    }

    /// The document's top level. Its id is 0.
    pub fn top_scope(&self, nodes: &'a [Node]) -> Scope<'a> {
        Scope { id: 0, graph: None, nodes, interface: HashMap::new(), user: true, path: String::new() }
    }

    /// A user nodegraph referenced by name (`nodegraph=`). Entered once: every
    /// reference shares the scope, so its nodes and uniforms are made once.
    /// Interface inputs bind to the graph's declared values.
    pub fn user_graph_scope(&mut self, name: &str, at: &Location) -> Result<Scope<'a>, Error> {
        if let Some(scope) = self.user_scopes.get(name) {
            return Ok(scope.clone());
        }
        let graph = self.index.nodegraph(name).ok_or_else(|| Error::BadValue {
            at: at.clone(),
            message: format!("no nodegraph named {name}"),
        })?;
        let id = self.new_scope_id();
        let mut interface = HashMap::new();
        for port in &graph.interface {
            let port_at = Location { file: self.file.clone(), line: port.line, path: format!("{name}/{}", port.name) };
            let binding = match (&port.port_type, &port.value, &port.default_geomprop) {
                (PortType::String | PortType::Filename, Some(text), _) => Binding::Text(text.clone()),
                (PortType::Value(ty), Some(text), _) if !text.is_empty() => {
                    let ty = self.value_type(&PortType::Value(*ty), &port_at)?;
                    let scope = Scope { id, graph: Some(graph), nodes: &graph.nodes, interface: HashMap::new(), user: true, path: name.to_string() };
                    Binding::Node(self.authored_value(&scope, &port.name, ty, text, true, &port_at)?)
                }
                (PortType::Value(ty), _, Some(geomprop)) => {
                    let ty = self.value_type(&PortType::Value(*ty), &port_at)?;
                    Binding::Node(self.geomprop_node(geomprop, ty, &port_at)?)
                }
                _ => Binding::Unbound,
            };
            interface.insert(port.name.clone(), binding);
        }
        let scope = Scope { id, graph: Some(graph), nodes: &graph.nodes, interface, user: true, path: name.to_string() };
        self.user_scopes.insert(name.to_string(), scope.clone());
        Ok(scope)
    }
}
