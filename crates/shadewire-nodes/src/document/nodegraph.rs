use super::node::{Node, build_node};
use super::nodedef::{PortDef, build_ports};
use super::port_type::PortType;

use crate::document::inherit::{Inherit, own};
use crate::element::Element;

#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub name: String,
    pub port_type: PortType,
    pub nodename: Option<String>,
    pub nodegraph: Option<String>,
    pub output: Option<String>,
    pub interfacename: Option<String>,
    pub line: u32,
    /// Where the host authored it (a builder's `source_path`); shown in messages in place of the line.
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeGraph {
    pub name: String,
    pub nodedef: Option<String>,
    pub interface: Vec<PortDef>,
    pub nodes: Vec<Node>,
    pub outputs: Vec<Output>,
    pub line: u32,
    /// Where the host authored it (a builder's `source_path`); shown in messages in place of the line.
    pub source: Option<String>,
}

pub fn build_output(element: &Element) -> Output {
    Output {
        name: element.name().to_string(),
        port_type: PortType::parse(element.attr("type").unwrap_or("")),
        nodename: own(element.attr("nodename")),
        nodegraph: own(element.attr("nodegraph")),
        output: own(element.attr("output")),
        interfacename: own(element.attr("interfacename")),
        line: element.line,
        source: None,
    }
}

pub fn build_nodegraph(element: &Element, inherit: &Inherit) -> NodeGraph {
    let inherit = inherit.enter(element);
    NodeGraph {
        name: element.name().to_string(),
        nodedef: own(element.attr("nodedef")),
        interface: build_ports(element, "input", &inherit),
        nodes: element
            .children
            .iter()
            .filter(|c| c.category != "input" && c.category != "output")
            .map(|c| build_node(c, &inherit))
            .collect(),
        outputs: element.children.iter().filter(|c| c.category == "output").map(build_output).collect(),
        line: element.line,
        source: None,
    }
}
