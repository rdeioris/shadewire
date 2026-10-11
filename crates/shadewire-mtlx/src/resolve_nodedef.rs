//! Picks the nodedef that implements a node.

use shadewire_ir::{Error, Location};

use crate::document::{Node, NodeDef, PortType};
use crate::index::Index;

/// `input_types` holds the resolved type of every input the node gives. An
/// empty-typed input (`Other("")`) matches a port of any type.
pub fn resolve_nodedef<'a>(
    index: &Index<'a>,
    node: &Node,
    input_types: &[(String, PortType)],
    at: &Location,
) -> Result<&'a NodeDef, Error> {
    let no_match = || Error::NoMatchingNodeDef {
        at: at.clone(),
        category: node.category.clone(),
        output: format!("{:?}", node.port_type),
    };

    if let Some(name) = &node.nodedef {
        return index.nodedef(name).ok_or_else(no_match);
    }

    let mut best: Option<(&'a NodeDef, usize)> = None;
    for candidate in index.nodedefs_for_category(&node.category) {
        if !output_matches(candidate, &node.port_type) || !inputs_match(candidate, input_types) {
            continue;
        }
        let unspecified = candidate.inputs.len() - input_types.len().min(candidate.inputs.len());
        // Strictly fewer wins, so ties keep the first in library order.
        if best.is_none_or(|(_, fewest)| unspecified < fewest) {
            best = Some((candidate, unspecified));
        }
    }
    best.map(|(nodedef, _)| nodedef).ok_or_else(no_match)
}

fn output_matches(nodedef: &NodeDef, node_type: &PortType) -> bool {
    if *node_type == PortType::MultiOutput {
        return true;
    }
    nodedef.outputs.first().is_some_and(|o| o.port_type == *node_type)
}

fn inputs_match(nodedef: &NodeDef, input_types: &[(String, PortType)]) -> bool {
    input_types.iter().all(|(name, ty)| {
        nodedef
            .inputs
            .iter()
            .find(|p| p.name == *name)
            .is_some_and(|p| *ty == PortType::Other(String::new()) || p.port_type == *ty)
    })
}

#[cfg(test)]
mod tests {
    use super::resolve_nodedef;
    use crate::document::build::build;
    use crate::document::{Document, Node, PortType};
    use crate::index::Index;
    use crate::library;
    use crate::parse::parse_str;
    use shadewire_ir::{Error, Location, Type};

    fn node(xml: &str) -> Node {
        let text = format!(r#"<materialx version="1.39">{xml}</materialx>"#);
        let root = parse_str(&text, None).unwrap();
        build(&root, None).unwrap().nodes.remove(0)
    }

    fn at() -> Location {
        Location { file: None, line: 1, path: "test".to_string() }
    }

    fn resolve(n: &Node, given: &[(&str, PortType)]) -> Result<String, Error> {
        let user = Document::empty();
        let index = Index::new(&user, library::standard());
        let given: Vec<(String, PortType)> =
            given.iter().map(|(k, t)| (k.to_string(), t.clone())).collect();
        resolve_nodedef(&index, n, &given, &at()).map(|d| d.name.clone())
    }

    fn color3() -> PortType {
        PortType::Value(Type::Color3)
    }

    fn float() -> PortType {
        PortType::Value(Type::Float)
    }

    #[test]
    fn color3_plus_float_is_the_fa_variant() {
        let n = node(r#"<add name="a" type="color3"/>"#);
        let got = resolve(&n, &[("in1", color3()), ("in2", float())]);
        assert_eq!(got.unwrap(), "ND_add_color3FA");
    }

    #[test]
    fn color3_plus_color3_is_plain() {
        let n = node(r#"<add name="a" type="color3"/>"#);
        let got = resolve(&n, &[("in1", color3()), ("in2", color3())]);
        assert_eq!(got.unwrap(), "ND_add_color3");
    }

    #[test]
    fn explicit_nodedef_wins() {
        let n = node(r#"<add name="a" type="color3" nodedef="ND_add_color3"/>"#);
        assert_eq!(resolve(&n, &[]).unwrap(), "ND_add_color3");
    }

    #[test]
    fn explicit_nodedef_that_does_not_exist_fails() {
        let n = node(r#"<add name="a" type="color3" nodedef="ND_nope"/>"#);
        assert!(matches!(resolve(&n, &[]), Err(Error::NoMatchingNodeDef { .. })));
    }

    #[test]
    fn no_candidate_fails() {
        let n = node(r#"<add name="a" type="matrix44"/>"#);
        let got = resolve(&n, &[("in1", float())]);
        assert!(matches!(got, Err(Error::NoMatchingNodeDef { .. })));
    }

    #[test]
    fn no_inputs_ties_go_to_library_order() {
        // color3 and color3FA both have two inputs, so with nothing given the
        // tie is broken by library order, where ND_add_color3 comes first.
        let n = node(r#"<add name="a" type="color3"/>"#);
        assert_eq!(resolve(&n, &[]).unwrap(), "ND_add_color3");
    }

    #[test]
    fn untyped_input_matches_any_type() {
        let n = node(r#"<add name="a" type="color3"/>"#);
        let untyped = PortType::Other(String::new());
        assert!(resolve(&n, &[("in1", untyped)]).is_ok());
    }
}
