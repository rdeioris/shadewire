use super::port_type::PortType;

use crate::document::inherit::{own, Inherit};
use crate::element::Element;

/// A port of a nodedef or a nodegraph interface.
#[derive(Clone, Debug, PartialEq)]
pub struct PortDef {
    pub name: String,
    pub port_type: PortType,
    pub value: Option<String>,
    pub default_geomprop: Option<String>,
    /// A nodegraph interface input connected to a node (not supported by elaboration).
    pub nodename: Option<String>,
    pub uniform: bool,
    pub colorspace: Option<String>,
    pub unit: Option<String>,
    pub unittype: Option<String>,
    pub enum_values: Option<String>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeDef {
    pub name: String,
    pub node: String,
    pub outputs: Vec<PortDef>,
    pub inputs: Vec<PortDef>,
    pub line: u32,
}

pub fn build_port_def(element: &Element, inherit: &Inherit) -> PortDef {
    let port_type = PortType::parse(element.attr("type").unwrap_or(""));
    let (value, colorspace) =
        inherit.resolve(&port_type, element.attr("colorspace"), element.attr("value"));
    PortDef {
        name: element.name().to_string(),
        port_type,
        value,
        default_geomprop: own(element.attr("defaultgeomprop")),
        nodename: own(element.attr("nodename")),
        uniform: element.attr("uniform") == Some("true"),
        colorspace,
        unit: own(element.attr("unit")),
        unittype: own(element.attr("unittype")),
        enum_values: own(element.attr("enum")),
        line: element.line,
    }
}

pub fn build_ports(element: &Element, category: &str, inherit: &Inherit) -> Vec<PortDef> {
    element
        .children
        .iter()
        .filter(|c| c.category == category)
        .map(|c| build_port_def(c, inherit))
        .collect()
}

pub fn build_nodedef(element: &Element, inherit: &Inherit) -> NodeDef {
    let inherit = inherit.enter(element);
    NodeDef {
        name: element.name().to_string(),
        node: element.attr("node").unwrap_or("").to_string(),
        outputs: build_ports(element, "output", &inherit),
        inputs: build_ports(element, "input", &inherit),
        line: element.line,
    }
}

#[cfg(test)]
mod tests {
    use super::build_nodedef;
    use crate::document::inherit::Inherit;
    use crate::parse::parse_str;

    #[test]
    fn reads_ports() {
        let e = parse_str(
            r#"<nodedef name="ND_x" node="x"><input name="a" type="float" value="1" uniform="true"/><output name="out" type="float"/></nodedef>"#,
            None,
        )
        .unwrap();
        let d = build_nodedef(&e, &Inherit::default());
        assert_eq!(d.node, "x");
        assert!(d.inputs[0].uniform);
        assert_eq!(d.outputs[0].name, "out");
    }
}
