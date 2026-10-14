//! `switch` grew from 5 to 10 inputs: a constant `which` of 5 or more used to fall back to 0
//! (Version.cpp, `nodeCategory == "switch"`).

use super::scope::for_each_scope;
use shadewire::Type;
use shadewire_nodes::document::{Document, PortType};

pub fn apply(doc: &mut Document) {
    for_each_scope(doc, |nodes| {
        for node in nodes.iter_mut().filter(|n| n.category == "switch") {
            let Some(which) = node.inputs.iter_mut().find(|i| i.name == "which") else {
                continue;
            };
            let Some(text) = which.value.as_deref() else {
                continue;
            };
            let out_of_range = match which.port_type {
                PortType::Value(Type::Integer) => text.trim().parse::<i64>().is_ok_and(|v| v >= 5),
                PortType::Value(Type::Float) => text.trim().parse::<f32>().is_ok_and(|v| v >= 5.0),
                _ => false,
            };
            if out_of_range {
                which.value = Some("0".to_string());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::upgrade::test_doc::doc_138;

    fn which_after(ty: &str, value: &str) -> String {
        let mut doc = doc_138(&format!(
            r#"<switch name="s" type="float"><input name="in1" type="float" value="1"/><input name="which" type="{ty}" value="{value}"/></switch>"#
        ));
        super::apply(&mut doc);
        doc.nodes[0].inputs[1].value.clone().unwrap()
    }

    #[test]
    fn integer_above_range_falls_back_to_zero() {
        assert_eq!(which_after("integer", "7"), "0");
    }

    #[test]
    fn float_above_range_falls_back_to_zero() {
        assert_eq!(which_after("float", "5.0"), "0");
    }

    #[test]
    fn in_range_is_kept() {
        assert_eq!(which_after("integer", "4"), "4");
    }
}
