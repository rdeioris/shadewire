use super::node::{build_node, Node};
use super::nodedef::{build_ports, PortDef};
use super::port_type::PortType;

use crate::document::inherit::Inherit;
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeGraph {
    pub name: String,
    pub nodedef: Option<String>,
    pub interface: Vec<PortDef>,
    pub nodes: Vec<Node>,
    pub outputs: Vec<Output>,
    pub line: u32,
}

fn own(value: Option<&str>) -> Option<String> {
    value.map(str::to_string)
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
        outputs: element
            .children
            .iter()
            .filter(|c| c.category == "output")
            .map(build_output)
            .collect(),
        line: element.line,
    }
}
