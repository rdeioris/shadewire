//! `channels="..."` on an input became explicit nodes. As in Version.cpp, each such input gets a
//! legacy `swizzle` node first; the swizzle rule then turns it into extract/combine nodes.

use super::scope::{copy_input, disconnect, for_each_scope, new_node, unique_name, value_input};
use crate::document::{Document, Input, Node, PortType};
use shadewire_ir::Type;

pub fn apply(doc: &mut Document) {
    for_each_scope(doc, apply_scope);
}

fn apply_scope(nodes: &mut Vec<Node>) {
    let types: Vec<(String, PortType)> =
        nodes.iter().map(|n| (n.name.clone(), n.port_type.clone())).collect();
    let mut result: Vec<Node> = Vec::with_capacity(nodes.len());
    let original = std::mem::take(nodes);
    for mut node in original {
        let mut inserted: Vec<Node> = Vec::new();
        for index in 0..node.inputs.len() {
            let Some(channels) = node.inputs[index].channels.clone() else { continue };
            let upstream = upstream_type(&node.inputs[index], &types, &channels);
            let input_type = node.inputs[index].port_type.clone();
            if upstream == Type::Float && input_type == PortType::Value(Type::Float) {
                node.inputs[index].channels = None;
                continue;
            }
            let base = format!("{}_{}_channels", node.name, node.inputs[index].name);
            let name = unique_name(&format!("{base}_0"), &result, &inserted);
            let source = copy_input(&node.inputs[index], "in", PortType::Value(upstream));
            let line = node.inputs[index].line;
            let channels_input = value_input("channels", PortType::String, &channels, line);
            inserted.push(new_node("swizzle", &name, input_type, vec![source, channels_input], line));
            let input = &mut node.inputs[index];
            disconnect(input);
            input.nodename = Some(name);
        }
        result.extend(inserted);
        result.push(node);
    }
    *nodes = result;
}

/// The type feeding the channels: the connected node's, else guessed from the channel string.
fn upstream_type(input: &Input, types: &[(String, PortType)], channels: &str) -> Type {
    if let Some(name) = &input.nodename {
        let found = types.iter().find(|(n, _)| n == name);
        if let Some((_, PortType::Value(ty))) = found {
            return *ty;
        }
    }
    match channels {
        "xx" | "xxx" | "xxxx" => Type::Float,
        "xyz" | "x" | "y" | "z" => Type::Vector3,
        "rgba" | "a" => Type::Color4,
        _ if input.port_type == PortType::Value(Type::Color3) => Type::Color4,
        _ => Type::Color3,
    }
}

#[cfg(test)]
mod tests {
    use crate::document::PortType;
    use crate::upgrade::test_doc::doc_138;
    use shadewire_ir::Type;

    const COLOR_SOURCE: &str =
        r#"<constant name="c" type="color3"><input name="value" type="color3" value="1,2,3"/></constant>"#;

    #[test]
    fn channels_attribute_becomes_a_swizzle_node() {
        let mut doc = doc_138(&format!(
            r#"{COLOR_SOURCE}<multiply name="m" type="float"><input name="in1" type="float" nodename="c" channels="g"/></multiply>"#
        ));
        super::apply(&mut doc);
        let names: Vec<&str> = doc.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["c", "m_in1_channels_0", "m"]);
        let swizzle = &doc.nodes[1];
        assert_eq!(swizzle.category, "swizzle");
        assert_eq!(swizzle.port_type, PortType::Value(Type::Float));
        let source = &swizzle.inputs[0];
        assert_eq!((source.name.as_str(), source.nodename.as_deref()), ("in", Some("c")));
        assert_eq!(source.port_type, PortType::Value(Type::Color3));
        assert_eq!(source.channels, None);
        assert_eq!(swizzle.inputs[1].value.as_deref(), Some("g"));
        let rewired = &doc.nodes[2].inputs[0];
        assert_eq!(rewired.nodename.as_deref(), Some("m_in1_channels_0"));
        assert_eq!(rewired.channels, None);
    }

    #[test]
    fn scalar_to_scalar_channels_are_dropped() {
        let mut doc = doc_138(
            r#"<constant name="c" type="float"><input name="value" type="float" value="1"/></constant>
               <multiply name="m" type="float"><input name="in1" type="float" nodename="c" channels="x"/></multiply>"#,
        );
        super::apply(&mut doc);
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.nodes[1].inputs[0].channels, None);
        assert_eq!(doc.nodes[1].inputs[0].nodename.as_deref(), Some("c"));
    }

    #[test]
    fn unknown_upstream_type_comes_from_the_channel_pattern() {
        let mut doc = doc_138(
            r#"<multiply name="m" type="float"><input name="in1" type="float" interfacename="p" channels="a"/></multiply>"#,
        );
        super::apply(&mut doc);
        let source = &doc.nodes[0].inputs[0];
        assert_eq!(source.port_type, PortType::Value(Type::Color4));
        assert_eq!(source.interfacename.as_deref(), Some("p"));
        assert_eq!(doc.nodes[1].inputs[0].interfacename, None);
    }

    #[test]
    fn graph_nodes_are_rewritten_too() {
        let mut doc = doc_138(
            r#"<nodegraph name="g"><multiply name="m" type="float"><input name="in1" type="float" interfacename="p" channels="xyz"/></multiply></nodegraph>"#,
        );
        super::apply(&mut doc);
        assert_eq!(doc.nodegraphs[0].nodes.len(), 2);
        assert_eq!(doc.nodegraphs[0].nodes[0].inputs[0].port_type, PortType::Value(Type::Vector3));
    }
}
