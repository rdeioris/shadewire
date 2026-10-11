//! `atan2` inputs `in1`, `in2` became `iny`, `inx` (Version.cpp, `nodeCategory == "atan2"`).

use super::scope::for_each_scope;
use crate::document::{Document, Node};

pub fn apply(doc: &mut Document) {
    for_each_scope(doc, |nodes| {
        for node in nodes.iter_mut().filter(|n| n.category == "atan2") {
            rename(node);
        }
    });
}

fn rename(node: &mut Node) {
    for input in &mut node.inputs {
        match input.name.as_str() {
            "in1" => input.name = "iny".to_string(),
            "in2" => input.name = "inx".to_string(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::upgrade::test_doc::doc_138;

    #[test]
    fn atan2_inputs_are_renamed() {
        let mut doc = doc_138(
            r#"<atan2 name="a" type="float"><input name="in1" type="float" value="1"/><input name="in2" type="float" nodename="b"/></atan2>
               <constant name="b" type="float"><input name="value" type="float" value="2"/></constant>"#,
        );
        super::apply(&mut doc);
        let a = doc.nodes.iter().find(|n| n.name == "a").unwrap();
        let names: Vec<&str> = a.inputs.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["iny", "inx"]);
        assert_eq!(a.inputs[1].nodename.as_deref(), Some("b"));
    }

    #[test]
    fn other_nodes_keep_in1() {
        let mut doc = doc_138(r#"<add name="m" type="float"><input name="in1" type="float" value="1"/></add>"#);
        super::apply(&mut doc);
        assert_eq!(doc.nodes[0].inputs[0].name, "in1");
    }
}
