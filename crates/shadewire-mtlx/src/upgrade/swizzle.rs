//! Legacy `swizzle` nodes became constant, extract, convert or combine nodes
//! (Version.cpp, `nodeCategory == "swizzle"`).
//!
//! Version.cpp builds the general case from a `separate` node with named outputs. This port
//! uses one `extract` per channel instead, so no multi-output node enters the graph.

use super::scope::{copy_input, for_each_scope, is_connected, new_node, node_input, unique_name, value_input};
use crate::document::{Document, Input, Node, PortType};
use shadewire_ir::Type;

/// Channel letters that read a component, by position.
fn channel_index(letter: char) -> Option<usize> {
    match letter {
        'r' | 'x' => Some(0),
        'g' | 'y' => Some(1),
        'b' | 'z' => Some(2),
        'a' | 'w' => Some(3),
        _ => None,
    }
}

fn channel_constant(letter: char) -> Option<&'static str> {
    match letter {
        '0' => Some("0"),
        '1' => Some("1"),
        _ => None,
    }
}

/// Patterns that are a plain `convert`, by channel string and source component count.
fn is_convert_pattern(channels: &str, source_count: usize) -> bool {
    matches!(
        (channels, source_count),
        ("rgb", 3)
            | ("rgb", 4)
            | ("rgba", 4)
            | ("xyz", 3)
            | ("xyz", 4)
            | ("xyzw", 4)
            | ("rr", 1)
            | ("rrr", 1)
            | ("xx", 1)
            | ("xxx", 1)
    )
}

fn swizzle_types(node: &Node) -> Option<(Type, Type)> {
    let source = match node.inputs.iter().find(|i| i.name == "in") {
        Some(input) => match input.port_type {
            PortType::Value(ty) => ty,
            _ => return None,
        },
        None => Type::Float,
    };
    let PortType::Value(dest) = node.port_type else { return None };
    let plain = |t: Type| !matches!(t, Type::Boolean | Type::Integer | Type::Matrix33 | Type::Matrix44);
    (plain(source) && plain(dest)).then_some((source, dest))
}

pub fn apply(doc: &mut Document) {
    for_each_scope(doc, apply_scope);
}

fn apply_scope(nodes: &mut Vec<Node>) {
    let mut result: Vec<Node> = Vec::with_capacity(nodes.len());
    let original = std::mem::take(nodes);
    for mut node in original {
        if node.category == "swizzle" {
            if let Some((source, dest)) = swizzle_types(&node) {
                let mut helpers = Vec::new();
                convert(&mut node, source, dest, &result, &mut helpers);
                result.extend(helpers);
            }
        }
        result.push(node);
    }
    *nodes = result;
}

fn convert(node: &mut Node, source: Type, dest: Type, before: &[Node], helpers: &mut Vec<Node>) {
    let channels = take_input(node, "channels").and_then(|i| i.value).unwrap_or_default();
    let letters: Vec<char> = channels.chars().collect();
    let dest_count = dest.component_count();
    let mut source_input = take_input(node, "in");
    if let Some(input) = &mut source_input {
        if input.value.is_some() && is_connected(input) {
            input.value = None;
        }
    }
    let all_constant = (0..dest_count).all(|i| letters.get(i).is_some_and(|l| channel_constant(*l).is_some()));
    let constant = source_input.as_ref().is_none_or(|i| i.value.is_some()) || all_constant;
    let Some(input) = source_input.clone().filter(|_| !constant) else {
        return to_constant(node, source_input.as_ref(), &letters, dest);
    };
    if dest_count == 1 {
        to_extract(node, input, &letters, source);
    } else if source != dest && is_convert_pattern(&channels, source.component_count()) {
        node.category = "convert".to_string();
        node.nodedef = node.nodedef.as_ref().map(|_| format!("ND_convert_{}_{}", source.mtlx_name(), dest.mtlx_name()));
        node.inputs.push(input);
    } else if source.component_count() == 1 {
        let scalar = |name: &str| copy_input(&input, name, PortType::Value(Type::Float));
        to_combine(node, dest, &letters, scalar);
    } else {
        let extracts = make_extracts(node, &input, &letters, dest, before);
        let mut next = extracts.iter().map(|n| n.name.clone());
        let line = node.line;
        let by_extract = |name: &str| {
            node_input(name, PortType::Value(Type::Float), &next.next().unwrap_or_default(), line)
        };
        to_combine(node, dest, &letters, by_extract);
        helpers.extend(extracts);
    }
}

fn take_input(node: &mut Node, name: &str) -> Option<Input> {
    let at = node.inputs.iter().position(|i| i.name == name)?;
    Some(node.inputs.remove(at))
}

fn to_extract(node: &mut Node, input: Input, letters: &[char], source: Type) {
    node.category = "extract".to_string();
    node.nodedef = node.nodedef.as_ref().map(|_| format!("ND_extract_{}", source.mtlx_name()));
    node.inputs.push(input);
    if let Some(index) = letters.first().and_then(|l| channel_index(*l)) {
        let index = value_input("index", PortType::Value(Type::Integer), &index.to_string(), node.line);
        node.inputs.push(index);
    }
}

fn to_constant(node: &mut Node, source: Option<&Input>, letters: &[char], dest: Type) {
    let text = source.and_then(|i| i.value.clone()).unwrap_or_else(|| "0".to_string());
    let tokens: Vec<&str> =
        text.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).collect();
    let first = tokens.first().copied().unwrap_or("0");
    let values: Vec<&str> = (0..dest.component_count())
        .map(|i| {
            let letter = letters.get(i).copied();
            match (letter.and_then(channel_index), letter.and_then(channel_constant)) {
                (Some(index), _) => tokens.get(index).copied().unwrap_or(first),
                (None, Some(constant)) => constant,
                (None, None) => first,
            }
        })
        .collect();
    node.category = "constant".to_string();
    node.nodedef = node.nodedef.as_ref().map(|_| format!("ND_constant_{}", dest.mtlx_name()));
    node.inputs = vec![value_input("value", PortType::Value(dest), &values.join(", "), node.line)];
}

/// One float `extract` per non-constant destination channel. A missing or invalid channel
/// letter reads the first component, as Version.cpp does.
fn make_extracts(node: &Node, source: &Input, letters: &[char], dest: Type, before: &[Node]) -> Vec<Node> {
    let mut extracts: Vec<Node> = Vec::new();
    for i in 0..dest.component_count() {
        let letter = letters.get(i).copied();
        if letter.and_then(channel_constant).is_some() {
            continue;
        }
        let index = letter.and_then(channel_index).unwrap_or(0);
        let name = unique_name(&format!("{}_swizzle_{}", node.name, extracts.len()), before, &extracts);
        let input = copy_input(source, "in", source.port_type.clone());
        let index = value_input("index", PortType::Value(Type::Integer), &index.to_string(), node.line);
        extracts.push(new_node("extract", &name, PortType::Value(Type::Float), vec![input, index], node.line));
    }
    extracts
}

/// Turn `node` into `combineN`; `component(name)` makes each non-constant `inK` input.
fn to_combine(node: &mut Node, dest: Type, letters: &[char], mut component: impl FnMut(&str) -> Input) {
    let count = dest.component_count();
    node.category = format!("combine{count}");
    node.nodedef = node.nodedef.as_ref().map(|_| format!("ND_combine{count}_{}", dest.mtlx_name()));
    node.inputs.clear();
    for i in 0..count {
        let name = format!("in{}", i + 1);
        let input = match letters.get(i).copied().and_then(channel_constant) {
            Some(value) => value_input(&name, PortType::Value(Type::Float), value, node.line),
            None => component(&name),
        };
        node.inputs.push(input);
    }
}

#[cfg(test)]
mod tests {
    use crate::document::PortType;
    use crate::upgrade::test_doc::doc_138;
    use shadewire_ir::Type;

    const COLOR_SOURCE: &str =
        r#"<constant name="c" type="color3"><input name="value" type="color3" value="1,2,3"/></constant>"#;

    fn swizzle(out_type: &str, in_type: &str, channels: &str) -> crate::document::Document {
        let mut doc = doc_138(&format!(
            r#"<swizzle name="s" type="{out_type}"><input name="in" type="{in_type}" nodename="c"/><input name="channels" type="string" value="{channels}"/></swizzle>
               <constant name="c" type="{in_type}"><input name="value" type="{in_type}" value="1,2,3"/></constant>"#
        ));
        super::apply(&mut doc);
        doc
    }

    fn categories(doc: &crate::document::Document) -> Vec<&str> {
        doc.nodes.iter().map(|n| n.category.as_str()).collect()
    }

    #[test]
    fn swizzle_becomes_extract_and_combine() {
        let mut doc = doc_138(&format!(
            r#"<swizzle name="s" type="color3"><input name="in" type="color3" nodename="c"/><input name="channels" type="string" value="bgr"/></swizzle>{COLOR_SOURCE}"#
        ));
        super::apply(&mut doc);
        let categories = categories(&doc);
        assert!(categories.contains(&"combine3"));
        assert_eq!(categories.iter().filter(|c| **c == "extract").count(), 3);
        assert!(!categories.contains(&"swizzle"));
        let combine = doc.nodes.iter().find(|n| n.name == "s").unwrap();
        let sources: Vec<&str> = combine.inputs.iter().map(|i| i.nodename.as_deref().unwrap()).collect();
        assert_eq!(sources, ["s_swizzle_0", "s_swizzle_1", "s_swizzle_2"]);
        let indices: Vec<&str> = (0..3)
            .map(|k| {
                let extract = doc.nodes.iter().find(|n| n.name == format!("s_swizzle_{k}")).unwrap();
                assert_eq!(extract.port_type, PortType::Value(Type::Float));
                assert_eq!(extract.inputs[0].nodename.as_deref(), Some("c"));
                extract.inputs[1].value.as_deref().unwrap()
            })
            .collect();
        assert_eq!(indices, ["2", "1", "0"]);
    }

    #[test]
    fn single_channel_becomes_extract_with_index() {
        let doc = swizzle("float", "color3", "g");
        let s = doc.nodes.iter().find(|n| n.name == "s").unwrap();
        assert_eq!(s.category, "extract");
        let names: Vec<&str> = s.inputs.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["in", "index"]);
        assert_eq!(s.inputs[1].value.as_deref(), Some("1"));
        assert_eq!(s.inputs[1].port_type, PortType::Value(Type::Integer));
    }

    #[test]
    fn widening_pattern_becomes_convert() {
        let doc = swizzle("color4", "color3", "rgb");
        let s = doc.nodes.iter().find(|n| n.name == "s").unwrap();
        assert_eq!(s.category, "convert");
        assert_eq!(s.inputs.len(), 1);
    }

    #[test]
    fn scalar_source_becomes_combine_of_the_scalar() {
        let mut doc = doc_138(
            r#"<swizzle name="s" type="color3"><input name="in" type="float" nodename="f"/><input name="channels" type="string" value="x0x"/></swizzle>
               <constant name="f" type="float"><input name="value" type="float" value="1"/></constant>"#,
        );
        super::apply(&mut doc);
        let s = doc.nodes.iter().find(|n| n.name == "s").unwrap();
        assert_eq!(s.category, "combine3");
        assert_eq!(s.inputs[0].nodename.as_deref(), Some("f"));
        assert_eq!(s.inputs[1].value.as_deref(), Some("0"));
        assert_eq!(s.inputs[2].nodename.as_deref(), Some("f"));
        assert_eq!(doc.nodes.len(), 2);
    }

    #[test]
    fn constant_input_becomes_constant_node() {
        let mut doc = doc_138(
            r#"<swizzle name="s" type="vector3"><input name="in" type="vector3" value="4, 5, 6"/><input name="channels" type="string" value="z1x"/></swizzle>"#,
        );
        super::apply(&mut doc);
        let s = &doc.nodes[0];
        assert_eq!(s.category, "constant");
        assert_eq!(s.inputs.len(), 1);
        assert_eq!(s.inputs[0].name, "value");
        assert_eq!(s.inputs[0].value.as_deref(), Some("6, 1, 4"));
    }

    #[test]
    fn all_constant_channels_ignore_a_connected_input() {
        let doc = swizzle("color3", "color3", "010");
        let s = doc.nodes.iter().find(|n| n.name == "s").unwrap();
        assert_eq!(s.category, "constant");
        assert_eq!(s.inputs[0].value.as_deref(), Some("0, 1, 0"));
    }

    #[test]
    fn nodedef_names_follow_the_new_category() {
        let mut doc = doc_138(
            r#"<swizzle name="s" type="float" nodedef="ND_swizzle_color3_float"><input name="in" type="color3" nodename="c"/><input name="channels" type="string" value="r"/></swizzle>
               <constant name="c" type="color3"><input name="value" type="color3" value="1,2,3"/></constant>"#,
        );
        super::apply(&mut doc);
        assert_eq!(doc.nodes[0].nodedef.as_deref(), Some("ND_extract_color3"));
    }
}
