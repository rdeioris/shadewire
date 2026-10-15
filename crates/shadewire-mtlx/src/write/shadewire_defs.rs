//! The declarations of Shadewire's own nodedefs, embedded in a document that uses them so other MaterialX tools
//! parse it.

use shadewire_nodes::element::Element as Parsed;
use shadewire_nodes::library::shadewire_ir_defs_text;
use shadewire_nodes::parse::parse_str;

use super::xml::Element;

/// The `<nodedef>`s of Shadewire's `shadewire_ir.mtlx` whose node is one of `categories`, in the file's order.
pub fn shadewire_declarations(categories: &[&str]) -> Vec<Element> {
    // The library loaded the same text already: it parses.
    let root = parse_str(shadewire_ir_defs_text(), Some("shadewire/shadewire_ir.mtlx"))
        .expect("Shadewire's shadewire_ir.mtlx parses");
    root.children
        .iter()
        .filter(|e| e.category == "nodedef" && e.attr("node").is_some_and(|node| categories.contains(&node)))
        .map(copied)
        .collect()
}

/// A parsed element as a written one: its tag, attributes in source order, and children.
fn copied(parsed: &Parsed) -> Element {
    let mut element = Element::new(&parsed.category);
    for (key, value) in &parsed.attributes {
        element = element.attr(key, value.as_str());
    }
    element.children = parsed.children.iter().map(copied).collect();
    element
}

#[cfg(test)]
mod tests {
    use super::super::xml::to_text;
    use super::*;

    #[test]
    fn only_the_used_categories_are_declared_with_their_ports() {
        let declared = shadewire_declarations(&["shadewire_dpdx", "shadewire_hextile_weights"]);
        let names: Vec<&str> = declared.iter().map(|e| e.attrs[0].1.as_str()).collect();
        assert_eq!(
            names,
            [
                "ND_shadewire_dpdx_float",
                "ND_shadewire_dpdx_vector2",
                "ND_shadewire_dpdx_vector3",
                "ND_shadewire_dpdx_vector4",
                "ND_shadewire_dpdx_color3",
                "ND_shadewire_dpdx_color4",
                "ND_shadewire_hextile_weights_vector3",
            ]
        );
        let weights = to_text(&declared[6]);
        assert!(weights.contains(r#"<input name="falloff" type="float" value="0.5" />"#), "{weights}");
        assert!(weights.contains(r#"<output name="out" type="vector3" />"#), "{weights}");
        assert!(shadewire_declarations(&[]).is_empty());
    }
}
