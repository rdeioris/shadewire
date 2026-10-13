//! Helpers shared by the rules: walk the node lists and make small inputs.

use crate::document::{Document, Input, Node, PortType};

/// Run `f` on the top-level node list and on every nodegraph's node list.
pub fn for_each_scope(doc: &mut Document, mut f: impl FnMut(&mut Vec<Node>)) {
    f(&mut doc.nodes);
    for graph in &mut doc.nodegraphs {
        f(&mut graph.nodes);
    }
}

/// `base` if no node in either list has that name, otherwise `base_1`, `base_2`, ...
pub fn unique_name(base: &str, nodes: &[Node], pending: &[Node]) -> String {
    let taken = |name: &str| nodes.iter().chain(pending).any(|n| n.name == name);
    if !taken(base) {
        return base.to_string();
    }
    let mut k = 1;
    loop {
        let name = format!("{base}_{k}");
        if !taken(&name) {
            return name;
        }
        k += 1;
    }
}

/// A value input with no connection.
pub fn value_input(name: &str, port_type: PortType, value: &str, line: u32) -> Input {
    Input {
        name: name.to_string(),
        port_type,
        value: Some(value.to_string()),
        nodename: None,
        nodegraph: None,
        output: None,
        interfacename: None,
        channels: None,
        colorspace: None,
        unit: None,
        unittype: None,
        ui: Default::default(),
        line,
    }
}

/// An input connected to the node `nodename`.
pub fn node_input(name: &str, port_type: PortType, nodename: &str, line: u32) -> Input {
    Input { value: None, nodename: Some(nodename.to_string()), ..value_input(name, port_type, "", line) }
}

/// A copy of `source` (value and every connection) under another name and type, with no channels.
pub fn copy_input(source: &Input, name: &str, port_type: PortType) -> Input {
    Input { name: name.to_string(), port_type, channels: None, ..source.clone() }
}

/// Drop the connection, value and channels of `input`.
pub fn disconnect(input: &mut Input) {
    input.value = None;
    input.nodename = None;
    input.nodegraph = None;
    input.output = None;
    input.interfacename = None;
    input.channels = None;
}

pub fn is_connected(input: &Input) -> bool {
    input.nodename.is_some() || input.nodegraph.is_some() || input.interfacename.is_some()
}

pub fn new_node(category: &str, name: &str, port_type: PortType, inputs: Vec<Input>, line: u32) -> Node {
    Node {
        category: category.to_string(),
        name: name.to_string(),
        port_type,
        nodedef: None,
        version: None,
        inputs,
        line,
    }
}
