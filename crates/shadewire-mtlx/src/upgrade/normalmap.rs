//! `normalmap` lost its `space` input (tangent space only; object space is refused).

use super::scope::{copy_input, for_each_scope, new_node, node_input, unique_name};
use shadewire::{Error, Location, Type};
use shadewire_nodes::document::{Document, Node, PortType};

const VECTOR3: PortType = PortType::Value(Type::Vector3);

pub fn apply(doc: &mut Document) -> Result<(), Error> {
    let file = doc.file.clone();
    let mut failure = None;
    for_each_scope(doc, |nodes| {
        if failure.is_none() {
            failure = apply_scope(nodes, &file).err();
        }
    });
    failure.map_or(Ok(()), Err)
}

fn apply_scope(nodes: &mut Vec<Node>, file: &Option<String>) -> Result<(), Error> {
    let original = std::mem::take(nodes);
    let mut result: Vec<Node> = Vec::with_capacity(original.len());
    for mut node in original {
        if node.category == "normalmap" {
            let space = node.inputs.iter().position(|i| i.name == "space");
            let object_space = space.is_some_and(|at| node.inputs[at].value.as_deref() == Some("object"));
            if object_space {
                let at = Location { file: file.clone(), line: node.line, path: node.name.clone() };
                return Err(Error::UnsupportedType { at, ty: "normalmap with space=\"object\"".to_string() });
            }
            if let Some(at) = space {
                node.inputs.remove(at);
            }
            add_bitangent(&mut node, &mut result);
            if node.nodedef.as_deref() == Some("ND_normalmap") {
                node.nodedef = Some("ND_normalmap_float".to_string());
            }
        }
        result.push(node);
    }
    *nodes = result;
    Ok(())
}

/// With a normal or tangent but no bitangent, the bitangent is `normalize(cross(N, T))`.
fn add_bitangent(node: &mut Node, result: &mut Vec<Node>) {
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
    let cross_name = unique_name("normalmap_cross", result, &[]);
    let cross = new_node("crossproduct", &cross_name, VECTOR3, cross_inputs, line);
    result.push(cross);
    let norm_name = unique_name("normalmap_cross_norm", result, &[]);
    let norm_input = node_input("in", VECTOR3, &cross_name, line);
    result.push(new_node("normalize", &norm_name, VECTOR3, vec![norm_input], line));
    node.inputs.push(node_input("bitangent", VECTOR3, &norm_name, line));
}

#[cfg(test)]
mod tests {
    use crate::upgrade::test_doc::doc_138;
    use shadewire::Error;

    #[test]
    fn tangent_space_input_is_removed() {
        let mut doc = doc_138(
            r#"<normalmap name="n" type="vector3" nodedef="ND_normalmap"><input name="in" type="vector3" value="0.5,0.5,1"/><input name="space" type="string" value="tangent"/></normalmap>"#,
        );
        super::apply(&mut doc).unwrap();
        let names: Vec<&str> = doc.nodes[0].inputs.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["in"]);
        assert_eq!(doc.nodes[0].nodedef.as_deref(), Some("ND_normalmap_float"));
        assert_eq!(doc.nodes.len(), 1);
    }

    #[test]
    fn object_space_is_unsupported_and_names_the_node() {
        let mut doc = doc_138(
            r#"<normalmap name="nm" type="vector3"><input name="space" type="string" value="object"/></normalmap>"#,
        );
        let err = super::apply(&mut doc).unwrap_err();
        match err {
            Error::UnsupportedType { at, ty } => {
                assert!(ty.contains("object"), "{ty}");
                assert_eq!(at.path, "nm");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn missing_bitangent_is_cross_of_normal_and_tangent() {
        let mut doc = doc_138(
            r#"<normalmap name="n" type="vector3"><input name="normal" type="vector3" nodename="nn"/><input name="tangent" type="vector3" nodename="tt"/></normalmap>"#,
        );
        super::apply(&mut doc).unwrap();
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
        super::apply(&mut doc).unwrap();
        assert_eq!(doc.nodes.len(), 1);
    }
}
