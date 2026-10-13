//! `subsurface_bsdf` input `radius` became a color3 (Version.cpp, `nodeCategory == "subsurface_bsdf"`): a value
//! changes type, a connection goes through a `convert` from vector3.

use super::scope::{copy_input, for_each_scope, new_node, node_input, unique_name};
use crate::document::{Document, Node, PortType};
use shadewire::Type;

const VECTOR3: PortType = PortType::Value(Type::Vector3);
const COLOR3: PortType = PortType::Value(Type::Color3);

pub fn apply(doc: &mut Document) {
    for_each_scope(doc, apply_scope);
}

fn apply_scope(nodes: &mut Vec<Node>) {
    let original = std::mem::take(nodes);
    let mut result: Vec<Node> = Vec::with_capacity(original.len());
    for mut node in original {
        if node.category == "subsurface_bsdf"
            && let Some(radius) = node.inputs.iter_mut().find(|i| i.name == "radius" && i.port_type == VECTOR3)
        {
            let connected = radius.nodename.is_some() || radius.nodegraph.is_some() || radius.interfacename.is_some();
            if connected {
                let name = unique_name(&format!("{}_radius_convert", node.name), &result, &[]);
                let input = copy_input(radius, "in", VECTOR3);
                result.push(new_node("convert", &name, COLOR3, vec![input], node.line));
                *radius = node_input("radius", COLOR3, &name, radius.line);
            } else {
                radius.port_type = COLOR3;
            }
        }
        result.push(node);
    }
    *nodes = result;
}

#[cfg(test)]
mod tests {
    use crate::document::PortType;
    use crate::upgrade::test_doc::doc_138;
    use shadewire::Type;

    #[test]
    fn a_radius_value_becomes_a_color() {
        let mut doc = doc_138(
            r#"<subsurface_bsdf name="s" type="BSDF"><input name="radius" type="vector3" value="0.3, 0.3, 0.3"/></subsurface_bsdf>"#,
        );
        super::apply(&mut doc);
        let radius = &doc.nodes[0].inputs[0];
        assert_eq!(
            (&radius.port_type, radius.value.as_deref()),
            (&PortType::Value(Type::Color3), Some("0.3, 0.3, 0.3"))
        );
    }

    #[test]
    fn a_radius_connection_goes_through_a_convert() {
        let mut doc = doc_138(
            r#"<constant name="r" type="vector3"><input name="value" type="vector3" value="1, 1, 1"/></constant>
               <subsurface_bsdf name="s" type="BSDF"><input name="radius" type="vector3" nodename="r"/></subsurface_bsdf>"#,
        );
        super::apply(&mut doc);
        let convert = doc.nodes.iter().find(|n| n.category == "convert").unwrap();
        assert_eq!(convert.inputs[0].nodename.as_deref(), Some("r"));
        let s = doc.nodes.iter().find(|n| n.name == "s").unwrap();
        assert_eq!(s.inputs[0].nodename.as_deref(), Some(convert.name.as_str()));
        assert_eq!(s.inputs[0].port_type, PortType::Value(Type::Color3));
    }
}
