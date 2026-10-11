use super::port_type::PortType;

use crate::element::Element;

#[derive(Clone, Debug, PartialEq)]
pub struct GeomPropDef {
    pub name: String,
    pub geomprop: String,
    pub port_type: PortType,
    pub space: Option<String>,
    pub index: Option<u32>,
}

pub fn build_geompropdef(element: &Element) -> GeomPropDef {
    GeomPropDef {
        name: element.name().to_string(),
        geomprop: element.attr("geomprop").unwrap_or("").to_string(),
        port_type: PortType::parse(element.attr("type").unwrap_or("")),
        space: element.attr("space").map(str::to_string),
        index: element.attr("index").and_then(|v| v.trim().parse().ok()),
    }
}
