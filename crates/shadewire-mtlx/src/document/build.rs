use shadewire::Error;

use super::Document;
use super::geompropdef::build_geompropdef;
use super::inherit::Inherit;
use super::node::build_node;
use super::nodedef::build_nodedef;
use super::nodegraph::{build_nodegraph, build_output};
use super::unitdef::build_unitdef;
use crate::element::Element;
use crate::version::document_version;

/// Build the typed document from the (include-resolved) root element.
pub fn build(root: &Element, file: Option<&str>) -> Result<Document, Error> {
    let mut doc = Document::empty();
    doc.version = document_version(root)?;
    doc.file = file.map(str::to_string);
    let inherit = Inherit::default().enter(root);
    for child in &root.children {
        match child.category.as_str() {
            "nodedef" => doc.nodedefs.push(build_nodedef(child, &inherit)),
            "nodegraph" => doc.nodegraphs.push(build_nodegraph(child, &inherit)),
            "output" => doc.outputs.push(build_output(child)),
            "unitdef" => doc.unitdefs.push(build_unitdef(child, file)?),
            "geompropdef" => doc.geompropdefs.push(build_geompropdef(child)),
            "unittypedef" | "typedef" | "implementation" | "look" | "token" | "materialassign" | "variantset"
            | "variantassign" | "visibility" | "collection" | "propertyset" | "propertysetassign" | "attributedef"
            | "lookgroup" => {}
            _ => doc.nodes.push(build_node(child, &inherit)),
        }
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::build;
    use crate::parse::parse_str;

    #[test]
    fn inherits_fileprefix_and_colorspace() {
        let text = r#"<materialx version="1.39" colorspace="lin_rec709" fileprefix="tex/">
          <nodegraph name="g">
            <image name="img" type="color3">
              <input name="file" type="filename" value="wood.png" colorspace="srgb_texture"/>
            </image>
            <constant name="c" type="color3"><input name="value" type="color3" value="1,0,0"/></constant>
          </nodegraph></materialx>"#;
        let doc = build(&parse_str(text, None).unwrap(), None).unwrap();
        let graph = &doc.nodegraphs[0];
        assert_eq!(graph.nodes[0].inputs[0].value.as_deref(), Some("tex/wood.png"));
        assert_eq!(graph.nodes[0].inputs[0].colorspace.as_deref(), Some("srgb_texture"));
        assert_eq!(graph.nodes[1].inputs[0].colorspace.as_deref(), Some("lin_rec709"));
    }

    #[test]
    fn builds_stdlib_defs() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/materialx/libraries/stdlib/stdlib_defs.mtlx");
        let text = std::fs::read_to_string(path).unwrap();
        let doc = build(&parse_str(&text, None).unwrap(), Some("stdlib_defs.mtlx")).unwrap();
        assert!(!doc.nodedefs.is_empty());
        assert!(!doc.unitdefs.is_empty());
        assert!(!doc.geompropdefs.is_empty());
        let add = doc.nodedefs.iter().find(|d| d.name == "ND_add_float").unwrap();
        let names: Vec<&str> = add.inputs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["in1", "in2"]);
    }

    fn doc(text: &str) -> crate::document::Document {
        build(&parse_str(text, None).unwrap(), None).unwrap()
    }

    #[test]
    fn node_colorspace_beats_graph_colorspace() {
        let d = doc(r#"<materialx colorspace="doc"><nodegraph name="g" colorspace="graph">
            <constant name="c" type="color3" colorspace="node"><input name="value" type="color3" value="1,0,0"/></constant>
            </nodegraph></materialx>"#);
        assert_eq!(d.nodegraphs[0].nodes[0].inputs[0].colorspace.as_deref(), Some("node"));
    }

    #[test]
    fn graph_colorspace_beats_document_colorspace() {
        let d = doc(r#"<materialx colorspace="doc"><nodegraph name="g" colorspace="graph">
            <constant name="c" type="color3"><input name="value" type="color3" value="1,0,0"/></constant>
            </nodegraph></materialx>"#);
        assert_eq!(d.nodegraphs[0].nodes[0].inputs[0].colorspace.as_deref(), Some("graph"));
    }

    #[test]
    fn graph_fileprefix_beats_document_fileprefix() {
        let d = doc(r#"<materialx fileprefix="doc/"><nodegraph name="g" fileprefix="graph/">
            <image name="i" type="color3"><input name="file" type="filename" value="a.png"/></image>
            </nodegraph></materialx>"#);
        assert_eq!(d.nodegraphs[0].nodes[0].inputs[0].value.as_deref(), Some("graph/a.png"));
    }

    #[test]
    fn fileprefix_skips_strings_and_float_has_no_colorspace() {
        let d = doc(r#"<materialx fileprefix="tex/" colorspace="doc"><nodegraph name="g">
            <n name="n" type="float">
              <input name="s" type="string" value="clamp"/>
              <input name="f" type="float" value="0.5"/>
            </n></nodegraph></materialx>"#);
        let inputs = &d.nodegraphs[0].nodes[0].inputs;
        assert_eq!(inputs[0].value.as_deref(), Some("clamp"));
        assert_eq!(inputs[1].colorspace, None);
    }

    #[test]
    fn nodegraph_outputs_and_interface() {
        let d = doc(r#"<materialx><nodegraph name="g" nodedef="ND_g">
            <input name="amount" type="float" value="2"/>
            <constant name="c" type="float"><input name="value" type="float" interfacename="amount"/></constant>
            <output name="out" type="float" nodename="c" output="o2"/>
            </nodegraph></materialx>"#);
        let g = &d.nodegraphs[0];
        assert_eq!(g.nodedef.as_deref(), Some("ND_g"));
        assert_eq!(g.interface[0].name, "amount");
        assert_eq!(g.interface[0].value.as_deref(), Some("2"));
        assert_eq!(g.outputs[0].nodename.as_deref(), Some("c"));
        assert_eq!(g.outputs[0].output.as_deref(), Some("o2"));
        assert_eq!(g.nodes[0].inputs[0].interfacename.as_deref(), Some("amount"));
    }

    #[test]
    fn stdlib_geompropdef_and_unit_scale() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/materialx/libraries/stdlib/stdlib_defs.mtlx");
        let text = std::fs::read_to_string(path).unwrap();
        let d = doc(&text);
        let uv0 = d.geompropdefs.iter().find(|g| g.name == "UV0").unwrap();
        assert_eq!(uv0.geomprop, "texcoord");
        assert_eq!(uv0.index, Some(0));
        let distance = d.unitdefs.iter().find(|u| u.unittype == "distance").unwrap();
        let cm = distance.scales.iter().find(|(n, _)| n == "centimeter").unwrap();
        assert_eq!(cm.1, 0.01);
    }
}
