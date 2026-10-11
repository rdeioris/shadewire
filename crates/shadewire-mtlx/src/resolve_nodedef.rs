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
    let given: Vec<String> = input_types.iter().map(|(name, ty)| format!("{name} {}", ty.mtlx_name())).collect();
    let no_match = || Error::NoMatchingNodeDef {
        at: at.clone(),
        category: node.category.clone(),
        output: node.port_type.mtlx_name(),
        inputs: if given.is_empty() { "none".into() } else { given.join(", ").into() },
    };

    if let Some(name) = &node.nodedef {
        return index.nodedef(name).ok_or_else(no_match).and_then(|nd| flattened(nd, at));
    }

    let mut best: Option<(&'a NodeDef, usize)> = None;
    for candidate in index.nodedefs_for_category(&node.category) {
        if !version_matches(candidate, node.version.as_deref())
            || !output_matches(candidate, &node.port_type)
            || !inputs_match(candidate, input_types)
        {
            continue;
        }
        let unspecified = candidate.inputs.len() - input_types.len().min(candidate.inputs.len());
        // Strictly fewer wins, so ties keep the first in library order.
        if best.is_none_or(|(_, fewest)| unspecified < fewest) {
            best = Some((candidate, unspecified));
        }
    }
    best.map(|(nodedef, _)| nodedef).ok_or_else(no_match).and_then(|nd| flattened(nd, at))
}

/// MaterialX `NodeDef::isVersionCompatible`: the same version string, or the
/// default version (or an unversioned nodedef) for a node that names none.
fn version_matches(nodedef: &NodeDef, wanted: Option<&str>) -> bool {
    match wanted {
        Some(version) => nodedef.version.as_deref() == Some(version),
        None => nodedef.version.is_none() || nodedef.is_default_version,
    }
}

/// Library nodedefs are flattened at load; phase 1 does not flatten a user
/// document's `inherit=`, so such a nodedef is refused rather than read short.
fn flattened<'a>(nodedef: &'a NodeDef, at: &Location) -> Result<&'a NodeDef, Error> {
    match &nodedef.inherits {
        Some(parent) => Err(Error::BadValue {
            at: at.clone(),
            message: format!(
                "nodedef {} inherits from {parent}; inherit= in a user document is not supported",
                nodedef.name
            ),
        }),
        None => Ok(nodedef),
    }
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
        let given: Vec<(String, PortType)> = given.iter().map(|(k, t)| (k.to_string(), t.clone())).collect();
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
    fn no_match_names_mtlx_types_and_the_given_inputs() {
        let n = node(r#"<add name="a" type="float"/>"#);
        let vector2 = PortType::Value(Type::Vector2);
        let err = resolve(&n, &[("in1", vector2), ("in2", float())]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "<string>:1 (test): no nodedef matches add -> float (inputs: in1 vector2, in2 float)"
        );
        let n = node(r#"<add name="a" type="boolean"/>"#);
        let err = resolve(&n, &[]).unwrap_err();
        assert_eq!(err.to_string(), "<string>:1 (test): no nodedef matches add -> boolean (inputs: none)");
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

    #[test]
    fn a_versionless_node_takes_the_default_version() {
        let n = node(r#"<standard_surface name="s" type="surfaceshader"/>"#);
        assert_eq!(resolve(&n, &[]).unwrap(), "ND_standard_surface_surfaceshader");
        let n = node(r#"<UsdUVTexture name="t" type="multioutput"/>"#);
        assert_eq!(resolve(&n, &[]).unwrap(), "ND_UsdUVTexture_23");
    }

    #[test]
    fn a_versioned_node_takes_that_version() {
        let n = node(r#"<standard_surface name="s" type="surfaceshader" version="1.0.0"/>"#);
        assert_eq!(resolve(&n, &[]).unwrap(), "ND_standard_surface_surfaceshader_100");
        let n = node(r#"<UsdUVTexture name="t" type="multioutput" version="2.2"/>"#);
        assert_eq!(resolve(&n, &[]).unwrap(), "ND_UsdUVTexture");
        let n = node(r#"<standard_surface name="s" type="surfaceshader" version="9.9"/>"#);
        assert!(matches!(resolve(&n, &[]), Err(Error::NoMatchingNodeDef { .. })));
    }

    #[test]
    fn a_user_nodedef_with_inherit_is_an_error_naming_it() {
        let text = r#"<materialx version="1.39">
            <nodedef name="ND_mine" node="mine" inherit="ND_add_float"><output name="out" type="float"/></nodedef>
            <mine name="m" type="float"/></materialx>"#;
        let user = build(&parse_str(text, None).unwrap(), None).unwrap();
        let index = Index::new(&user, library::standard());
        let err = resolve_nodedef(&index, &user.nodes[0], &[], &at()).unwrap_err();
        assert!(matches!(&err, Error::BadValue { message, .. } if message.contains("ND_mine")), "{err:?}");
    }
}
