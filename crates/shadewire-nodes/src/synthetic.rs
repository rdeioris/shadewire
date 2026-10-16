//! Nodes a front end makes up on the spot, kept alive as long as the elaborator that reads them.
//!
//! The elaborator borrows every node it reads for its whole life (`&'a Node`), so a front end that has no document
//! allocates its nodes here and lends the arena to `Elaborator::standalone`.

use std::cell::Cell;

use typed_arena::Arena;

use crate::document::{Input, Node, PortType, UiAttrs};

#[derive(Default)]
pub struct Synthetic {
    nodes: Arena<Node>,
    groups: Arena<Vec<Node>>,
    next: Cell<u32>,
}

impl Synthetic {
    pub fn new() -> Synthetic {
        Synthetic::default()
    }

    /// A node name no other synthetic node has: `s1`, `s2`, ...
    pub fn name(&self) -> String {
        self.next.set(self.next.get() + 1);
        format!("s{}", self.next.get())
    }

    pub fn node(&self, node: Node) -> &Node {
        self.nodes.alloc(node)
    }

    /// Sibling nodes that connect to one another by `nodename`: one scope's nodes.
    pub fn group(&self, nodes: Vec<Node>) -> &[Node] {
        self.groups.alloc(nodes)
    }
}

/// An input with nothing but a name and a type; the caller sets `value`, `nodename` or `interfacename`.
pub fn input(name: &str, port_type: PortType) -> Input {
    Input {
        name: name.to_string(),
        port_type,
        value: None,
        nodename: None,
        nodegraph: None,
        output: None,
        interfacename: None,
        channels: None,
        colorspace: None,
        unit: None,
        unittype: None,
        ui: UiAttrs::default(),
        line: 0,
        source: None,
    }
}
