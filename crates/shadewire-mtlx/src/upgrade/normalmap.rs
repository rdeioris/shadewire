//! `normalmap` lost its `space` input: 1.39 maps are tangent space only. An object-space map becomes
//! `normalize(in * 2 - 1)`, as upstream's upgrade writes it: the decoded vector, neither moved into the
//! tangent frame nor out of object space (1.38's genglsl `mx_normalmap` did no more).

use super::scope::{copy_input, for_each_scope, new_node, node_input, unique_name, value_input};
use crate::document::{Document, Node, PortType};
use shadewire::Type;

const VECTOR3: PortType = PortType::Value(Type::Vector3);

pub fn apply(doc: &mut Document) {
    for_each_scope(doc, apply_scope);
}

fn apply_scope(nodes: &mut Vec<Node>) {
    let original = std::mem::take(nodes);
    let mut result: Vec<Node> = Vec::with_capacity(original.len());
    // New nodes' names must not collide with the nodes still to come either.
    let later = original.clone();
    for mut node in original {
        if node.category == "normalmap" {
            let space = node.inputs.iter().position(|i| i.name == "space");
            let object_space = space.is_some_and(|at| node.inputs[at].value.as_deref() == Some("object"));
            if object_space {
                let normalize = object_space_graph(node, &mut result, &later);
                result.push(normalize);
                continue;
            }
            if let Some(at) = space {
                node.inputs.remove(at);
            }
            add_bitangent(&mut node, &mut result, &later);
            if node.nodedef.as_deref() == Some("ND_normalmap") {
                node.nodedef = Some("ND_normalmap_float".to_string());
            }
        }
        result.push(node);
    }
    *nodes = result;
}

/// The object-space `node` as `normalize(subtract(multiply(in, 2), 1))`, keeping its name so its readers
/// still find it; pushes the multiply and subtract to `result`.
fn object_space_graph(node: Node, result: &mut Vec<Node>, later: &[Node]) -> Node {
    let line = node.line;
    let float = PortType::Value(Type::Float);
    // An unset `in` keeps ND_normalmap's default; upstream's copy would leave the multiply's 0 instead.
    let input = match node.inputs.iter().find(|i| i.name == "in") {
        Some(given) => copy_input(given, "in1", VECTOR3),
        None => value_input("in1", VECTOR3, "0.5, 0.5, 1.0", line),
    };
    let multiply_name = unique_name("multiply", result, later);
    let two = value_input("in2", float.clone(), "2.0", line);
    result.push(new_node("multiply", &multiply_name, VECTOR3, vec![input, two], line));
    let subtract_name = unique_name("subtract", result, later);
    let decoded = node_input("in1", VECTOR3, &multiply_name, line);
    let one = value_input("in2", float, "1.0", line);
    result.push(new_node("subtract", &subtract_name, VECTOR3, vec![decoded, one], line));
    let nodedef = node.nodedef.as_ref().map(|_| "ND_normalize_vector3".to_string());
    Node {
        nodedef,
        version: None,
        ..new_node("normalize", &node.name, VECTOR3, vec![node_input("in", VECTOR3, &subtract_name, line)], line)
    }
}

/// With a normal or tangent but no bitangent, the bitangent is `normalize(cross(N, T))`.
fn add_bitangent(node: &mut Node, result: &mut Vec<Node>, later: &[Node]) {
    let find = |name: &str| node.inputs.iter().position(|i| i.name == name);
    let (normal, tangent) = (find("normal"), find("tangent"));
    if (normal.is_none() && tangent.is_none()) || find("bitangent").is_some() {
        return;
    }
    let mut cross_inputs = Vec::new();
    if let Some(at) = normal {
        cross_inputs.push(copy_input(&node.inputs[at], "in1", node.inputs[at].port_type.clone()));
    }
    if let Some(at) = tangent {
        cross_inputs.push(copy_input(&node.inputs[at], "in2", node.inputs[at].port_type.clone()));
    }
    let line = node.line;
    let cross_name = unique_name("normalmap_cross", result, later);
    let cross = new_node("crossproduct", &cross_name, VECTOR3, cross_inputs, line);
    result.push(cross);
    let norm_name = unique_name("normalmap_cross_norm", result, later);
    let norm_input = node_input("in", VECTOR3, &cross_name, line);
    result.push(new_node("normalize", &norm_name, VECTOR3, vec![norm_input], line));
    node.inputs.push(node_input("bitangent", VECTOR3, &norm_name, line));
}

#[cfg(test)]
mod tests {
    use crate::upgrade::test_doc::doc_138;

    #[test]
    fn tangent_space_input_is_removed() {
        let mut doc = doc_138(
            r#"<normalmap name="n" type="vector3" nodedef="ND_normalmap"><input name="in" type="vector3" value="0.5,0.5,1"/><input name="space" type="string" value="tangent"/></normalmap>"#,
        );
        super::apply(&mut doc);
        let names: Vec<&str> = doc.nodes[0].inputs.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["in"]);
        assert_eq!(doc.nodes[0].nodedef.as_deref(), Some("ND_normalmap_float"));
        assert_eq!(doc.nodes.len(), 1);
    }

    #[test]
    fn object_space_decodes_and_normalizes_under_the_same_name() {
        let mut doc = doc_138(
            r#"<normalmap name="nm" type="vector3" nodedef="ND_normalmap"><input name="in" type="vector3" nodename="tex"/><input name="space" type="string" value="object"/><input name="scale" type="float" value="2"/></normalmap>"#,
        );
        super::apply(&mut doc);
        let names: Vec<&str> = doc.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["multiply", "subtract", "nm"]);
        let (multiply, subtract, normalize) = (&doc.nodes[0], &doc.nodes[1], &doc.nodes[2]);
        assert_eq!(multiply.inputs[0].nodename.as_deref(), Some("tex"));
        assert_eq!(multiply.inputs[1].value.as_deref(), Some("2.0"));
        assert_eq!(subtract.inputs[0].nodename.as_deref(), Some("multiply"));
        assert_eq!(subtract.inputs[1].value.as_deref(), Some("1.0"));
        assert_eq!(
            (normalize.category.as_str(), normalize.nodedef.as_deref()),
            ("normalize", Some("ND_normalize_vector3"))
        );
        assert_eq!(normalize.inputs.len(), 1);
        assert_eq!(normalize.inputs[0].nodename.as_deref(), Some("subtract"));
    }

    #[test]
    fn new_names_avoid_later_nodes() {
        let mut doc = doc_138(
            r#"<normalmap name="nm" type="vector3"><input name="space" type="string" value="object"/></normalmap>
               <multiply name="multiply" type="float"/>"#,
        );
        super::apply(&mut doc);
        let names: Vec<&str> = doc.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["multiply_1", "subtract", "nm", "multiply"]);
    }

    #[test]
    fn object_space_without_an_input_decodes_the_default() {
        let mut doc = doc_138(
            r#"<normalmap name="nm" type="vector3"><input name="space" type="string" value="object"/></normalmap>"#,
        );
        super::apply(&mut doc);
        assert_eq!(doc.nodes[0].inputs[0].value.as_deref(), Some("0.5, 0.5, 1.0"));
        assert_eq!(doc.nodes[2].nodedef, None);
    }

    #[test]
    fn missing_bitangent_is_cross_of_normal_and_tangent() {
        let mut doc = doc_138(
            r#"<normalmap name="n" type="vector3"><input name="normal" type="vector3" nodename="nn"/><input name="tangent" type="vector3" nodename="tt"/></normalmap>"#,
        );
        super::apply(&mut doc);
        let names: Vec<&str> = doc.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["normalmap_cross", "normalmap_cross_norm", "n"]);
        let cross = &doc.nodes[0];
        assert_eq!(cross.category, "crossproduct");
        assert_eq!(cross.inputs[0].name, "in1");
        assert_eq!(cross.inputs[0].nodename.as_deref(), Some("nn"));
        assert_eq!(cross.inputs[1].name, "in2");
        assert_eq!(cross.inputs[1].nodename.as_deref(), Some("tt"));
        assert_eq!(doc.nodes[1].category, "normalize");
        let bitangent = doc.nodes[2].inputs.iter().find(|i| i.name == "bitangent").unwrap();
        assert_eq!(bitangent.nodename.as_deref(), Some("normalmap_cross_norm"));
    }

    #[test]
    fn existing_bitangent_is_kept() {
        let mut doc = doc_138(
            r#"<normalmap name="n" type="vector3"><input name="tangent" type="vector3" nodename="tt"/><input name="bitangent" type="vector3" nodename="bb"/></normalmap>"#,
        );
        super::apply(&mut doc);
        assert_eq!(doc.nodes.len(), 1);
    }
}
