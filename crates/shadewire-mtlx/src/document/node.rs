use super::port_type::PortType;

use crate::document::inherit::Inherit;
use crate::element::Element;

#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub name: String,
    pub port_type: PortType,
    pub value: Option<String>,
    pub nodename: Option<String>,
    pub nodegraph: Option<String>,
    pub output: Option<String>,
    pub interfacename: Option<String>,
    pub channels: Option<String>,
    pub colorspace: Option<String>,
    pub unit: Option<String>,
    pub unittype: Option<String>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub category: String,
    pub name: String,
    pub port_type: PortType,
    pub nodedef: Option<String>,
    pub inputs: Vec<Input>,
    pub line: u32,
}

fn own(value: Option<&str>) -> Option<String> {
    value.map(str::to_string)
}

/// `inherit` is the context of the node (document and graph already folded in).
pub fn build_input(element: &Element, inherit: &Inherit) -> Input {
    let port_type = PortType::parse(element.attr("type").unwrap_or(""));
    let colorspace = if port_type.has_colorspace() {
        own(element.attr("colorspace")).or_else(|| inherit.colorspace.clone())
    } else {
        None
    };
    let mut value = own(element.attr("value"));
    if port_type == PortType::Filename {
        value = value.map(|v| format!("{}{v}", inherit.fileprefix));
    }
    Input {
        name: element.name().to_string(),
        port_type,
        value,
        nodename: own(element.attr("nodename")),
        nodegraph: own(element.attr("nodegraph")),
        output: own(element.attr("output")),
        interfacename: own(element.attr("interfacename")),
        channels: own(element.attr("channels")),
        colorspace,
        unit: own(element.attr("unit")),
        unittype: own(element.attr("unittype")),
        line: element.line,
    }
}

pub fn build_node(element: &Element, inherit: &Inherit) -> Node {
    let inherit = inherit.enter(element);
    Node {
        category: element.category.clone(),
        name: element.name().to_string(),
        port_type: PortType::parse(element.attr("type").unwrap_or("")),
        nodedef: own(element.attr("nodedef")),
        inputs: element
            .children
            .iter()
            .filter(|c| c.category == "input")
            .map(|c| build_input(c, &inherit))
            .collect(),
        line: element.line,
    }
}
