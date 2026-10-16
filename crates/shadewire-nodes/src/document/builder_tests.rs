use shadewire::Error;
use shadewire::export::{ExportDocument, export};

use super::build::build;
use super::builder::{DocumentBuilder, GraphRef, InputValue as V, NodeRef, Source};
use super::{Node, PortType};
use crate::elaborate::elaborate;
use crate::library::standard;
use crate::options::IrOptions;
use crate::parse::parse_str;
use shadewire::Type;

/// The material of `TWIN_XML`, built through the builder: a nodegraph with interface inputs (one passed straight
/// through to an output), an image with a colorspace, a multi-output `separate3` read by output name, and a
/// `standard_surface` reading the graph's outputs.
fn twin() -> DocumentBuilder {
    let mut b = DocumentBuilder::new();
    let ng = b.add_nodegraph("NG", Some("/M/NG")).unwrap();
    b.add_interface_input(ng, "tint", "color3", Some(V::Floats(&[0.5, 0.25, 1.0])), None).unwrap();
    b.add_interface_input(ng, "rough", "float", Some(V::Floats(&[0.4])), None).unwrap();
    let img = b.add_node_from_nodedef(Some(ng), "ND_image_color3", "img", Some("/M/NG/img")).unwrap();
    b.set_input(img, "file", None, V::Text("wood.png"), None).unwrap();
    b.set_input_colorspace(img, "file", "srgb_texture").unwrap();
    b.set_input(img, "frameoffset", None, V::Integer(0), None).unwrap();
    let mul = b.add_node(Some(ng), "multiply", "color3", "mul", None).unwrap();
    b.connect(mul, "in1", Some("color3"), Source::Node(img, None), None).unwrap();
    b.connect(mul, "in2", Some("color3"), Source::Interface("tint"), None).unwrap();
    let sep = b.add_node_from_nodedef(Some(ng), "ND_separate3_color3", "sep", None).unwrap();
    b.connect(sep, "in", None, Source::Node(mul, None), None).unwrap();
    b.add_output(Some(ng), "base_color", "color3", Source::Node(mul, None), None).unwrap();
    b.add_output(Some(ng), "metal", "float", Source::Node(sep, Some("outr")), None).unwrap();
    b.add_output(Some(ng), "roughness", "float", Source::Interface("rough"), None).unwrap();
    let sr = b.add_node_from_nodedef(None, "ND_standard_surface_surfaceshader", "SR", None).unwrap();
    b.connect(sr, "base_color", None, Source::Graph(ng, "base_color"), None).unwrap();
    b.connect(sr, "metalness", None, Source::Graph(ng, "metal"), None).unwrap();
    b.connect(sr, "specular_roughness", None, Source::Graph(ng, "roughness"), None).unwrap();
    b.set_input(sr, "base", None, V::Floats(&[0.8]), None).unwrap();
    b.set_input(sr, "thin_walled", None, V::Boolean(true), None).unwrap();
    b.add_material("M", sr, None).unwrap();
    b
}

const TWIN_XML: &str = r#"<materialx version="1.39">
  <nodegraph name="NG">
    <input name="tint" type="color3" value="0.5, 0.25, 1"/>
    <input name="rough" type="float" value="0.4"/>
    <image name="img" type="color3" nodedef="ND_image_color3">
      <input name="file" type="filename" value="wood.png" colorspace="srgb_texture"/>
      <input name="frameoffset" type="integer" value="0"/>
    </image>
    <multiply name="mul" type="color3">
      <input name="in1" type="color3" nodename="img"/>
      <input name="in2" type="color3" interfacename="tint"/>
    </multiply>
    <separate3 name="sep" type="multioutput" nodedef="ND_separate3_color3">
      <input name="in" type="color3" nodename="mul"/>
    </separate3>
    <output name="base_color" type="color3" nodename="mul"/>
    <output name="metal" type="float" nodename="sep" output="outr"/>
    <output name="roughness" type="float" interfacename="rough"/>
  </nodegraph>
  <standard_surface name="SR" type="surfaceshader" nodedef="ND_standard_surface_surfaceshader">
    <input name="base_color" type="color3" nodegraph="NG" output="base_color"/>
    <input name="metalness" type="float" nodegraph="NG" output="metal"/>
    <input name="specular_roughness" type="float" nodegraph="NG" output="roughness"/>
    <input name="base" type="float" value="0.8"/>
    <input name="thin_walled" type="boolean" value="true"/>
  </standard_surface>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR"/>
  </surfacematerial>
</materialx>"#;

fn xml_ir(text: &str, options: &IrOptions) -> ExportDocument {
    let doc = build(&parse_str(text, None).unwrap(), None).unwrap();
    export(&elaborate(&doc, standard(), options, None).unwrap())
}

fn node<'d>(b: &'d DocumentBuilder, graph: Option<&str>, name: &str) -> &'d Node {
    let doc = b.document();
    let nodes = match graph {
        Some(g) => &doc.nodegraphs.iter().find(|n| n.name == g).unwrap().nodes,
        None => &doc.nodes,
    };
    nodes.iter().find(|n| n.name == name).unwrap()
}

fn input<'d>(node: &'d Node, name: &str) -> &'d super::Input {
    node.inputs.iter().find(|i| i.name == name).unwrap()
}

/// A builder with a nodegraph `NG` (interface input `k`, a float) holding a `multiply` `mul` and a `separate3`
/// `sep`, and a document-level `standard_surface` `SR`.
fn small() -> (DocumentBuilder, GraphRef, NodeRef, NodeRef, NodeRef) {
    let mut b = DocumentBuilder::new();
    let ng = b.add_nodegraph("NG", None).unwrap();
    b.add_interface_input(ng, "k", "float", None, None).unwrap();
    let mul = b.add_node(Some(ng), "multiply", "color3", "mul", None).unwrap();
    let sep = b.add_node_from_nodedef(Some(ng), "ND_separate3_color3", "sep", None).unwrap();
    let sr = b.add_node_from_nodedef(None, "ND_standard_surface_surfaceshader", "SR", None).unwrap();
    (b, ng, mul, sep, sr)
}

fn says(err: Error, parts: &[&str]) {
    let text = err.to_string();
    for part in parts {
        assert!(text.contains(part), "{part:?} not in {text:?}");
    }
}

#[test]
fn a_built_material_gives_the_ir_of_its_xml_twin() {
    let b = twin();
    for options in [
        IrOptions::default(),
        IrOptions::with_models(&["surface"]),
        IrOptions { values_as_uniforms: false, ..IrOptions::default() },
    ] {
        let built = export(&b.to_ir(Some("M"), &options).unwrap().dag);
        assert_eq!(built, xml_ir(TWIN_XML, &options), "{options:?}");
    }
}

#[test]
fn the_ir_of_a_built_material_says_where_it_came_from() {
    let ir = twin().to_ir(None, &IrOptions::default()).unwrap();
    assert_eq!(ir.source.source, "mtlx");
    assert_eq!(ir.source.mtlx_version, "1.39");
    assert!(ir.warnings.is_empty());
}

#[test]
fn every_source_kind_lands_in_the_document() {
    let b = twin();
    let mul = node(&b, Some("NG"), "mul");
    assert_eq!(input(mul, "in1").nodename.as_deref(), Some("img"));
    assert_eq!(input(mul, "in1").output, None);
    assert_eq!(input(mul, "in2").interfacename.as_deref(), Some("tint"));
    let sr = node(&b, None, "SR");
    assert_eq!(input(sr, "base_color").nodegraph.as_deref(), Some("NG"));
    assert_eq!(input(sr, "base_color").output.as_deref(), Some("base_color"));
    let graph = &b.document().nodegraphs[0];
    assert_eq!(graph.outputs[1].nodename.as_deref(), Some("sep"));
    assert_eq!(graph.outputs[1].output.as_deref(), Some("outr"));
    assert_eq!(graph.outputs[2].interfacename.as_deref(), Some("rough"));
    assert_eq!(graph.source.as_deref(), Some("/M/NG"));
    let material = node(&b, None, "M");
    assert_eq!(material.category, "surfacematerial");
    assert_eq!(material.port_type, PortType::Material);
    assert_eq!(input(material, "surfaceshader").nodename.as_deref(), Some("SR"));
    assert_eq!(input(material, "surfaceshader").port_type, PortType::SurfaceShader);
}

#[test]
fn a_nodedef_alone_names_the_category_and_the_output_type() {
    let b = twin();
    let img = node(&b, Some("NG"), "img");
    assert_eq!((img.category.as_str(), &img.port_type), ("image", &PortType::Value(Type::Color3)));
    assert_eq!(img.nodedef.as_deref(), Some("ND_image_color3"));
    assert_eq!(img.source.as_deref(), Some("/M/NG/img"));
    let sep = node(&b, Some("NG"), "sep");
    assert_eq!((sep.category.as_str(), &sep.port_type), ("separate3", &PortType::MultiOutput));
    let sr = node(&b, None, "SR");
    assert_eq!((sr.category.as_str(), &sr.port_type), ("standard_surface", &PortType::SurfaceShader));
}

#[test]
fn an_unknown_nodedef_is_an_error_naming_the_source_path() {
    let mut b = DocumentBuilder::new();
    let err = b.add_node_from_nodedef(None, "ND_nope", "n", Some("/root/Looks/n")).unwrap_err();
    says(err, &["/root/Looks/n", "ND_nope"]);
}

#[test]
fn a_multi_output_node_is_read_by_an_output_it_has() {
    let (mut b, ng, mul, sep, _) = small();
    let err = b.connect(mul, "in1", Some("color3"), Source::Node(sep, Some("outq")), Some("/NG/mul.inputs:in1"));
    says(err.unwrap_err(), &["/NG/mul.inputs:in1", "outq"]);
    let err = b.add_output(Some(ng), "o", "float", Source::Node(sep, Some("outq")), Some("/NG.outputs:o"));
    says(err.unwrap_err(), &["/NG.outputs:o", "outq"]);
    b.connect(mul, "in1", Some("color3"), Source::Node(sep, Some("outg")), None).unwrap();
}

#[test]
fn typed_values_are_written_as_materialx_text() {
    let b = twin();
    let graph = &b.document().nodegraphs[0];
    assert_eq!(graph.interface[0].value.as_deref(), Some("0.5, 0.25, 1"));
    assert_eq!(graph.interface[0].port_type, PortType::Value(Type::Color3));
    let img = node(&b, Some("NG"), "img");
    assert_eq!(input(img, "file").value.as_deref(), Some("wood.png"));
    assert_eq!(input(img, "file").port_type, PortType::Filename);
    assert_eq!(input(img, "frameoffset").value.as_deref(), Some("0"));
    assert_eq!(input(img, "frameoffset").port_type, PortType::Value(Type::Integer));
    let sr = node(&b, None, "SR");
    assert_eq!(input(sr, "thin_walled").value.as_deref(), Some("true"));
    assert_eq!(input(sr, "base").value.as_deref(), Some("0.8"));
}

#[test]
fn a_value_count_that_does_not_match_the_type_is_an_error() {
    let (mut b, ng, mul, _, _) = small();
    let err = b.set_input(mul, "in1", Some("color3"), V::Floats(&[1.0, 2.0]), Some("/NG/mul.inputs:in1"));
    says(err.unwrap_err(), &["/NG/mul.inputs:in1", "color3", "3", "2"]);
    let err = b.add_interface_input(ng, "v", "vector2", Some(V::Floats(&[1.0])), Some("/NG.inputs:v"));
    says(err.unwrap_err(), &["/NG.inputs:v", "vector2"]);
}

#[test]
fn a_value_of_the_wrong_kind_is_an_error() {
    let (mut b, _, mul, _, sr) = small();
    says(b.set_input(mul, "in1", Some("color3"), V::Integer(1), Some("/a")).unwrap_err(), &["/a", "color3"]);
    says(b.set_input(mul, "in1", Some("color3"), V::Text("x"), Some("/b")).unwrap_err(), &["/b", "color3"]);
    says(b.set_input(sr, "base", None, V::Boolean(true), Some("/c")).unwrap_err(), &["/c", "float"]);
    says(b.set_input(mul, "in1", Some("string"), V::Floats(&[1.0]), Some("/d")).unwrap_err(), &["/d", "string"]);
    says(b.set_input(mul, "in1", Some("nonsense"), V::Floats(&[1.0]), Some("/e")).unwrap_err(), &["/e", "nonsense"]);
}

#[test]
fn an_omitted_input_type_needs_a_nodedef_that_has_the_input() {
    let (mut b, _, mul, _, sr) = small();
    says(b.set_input(mul, "in1", None, V::Floats(&[1.0, 1.0, 1.0]), Some("/a")).unwrap_err(), &["/a", "in1"]);
    says(b.set_input(sr, "no_such_input", None, V::Floats(&[1.0]), Some("/b")).unwrap_err(), &["/b", "no_such_input"]);
    b.set_input(sr, "base_color", None, V::Floats(&[1.0, 0.5, 0.25]), None).unwrap();
    assert_eq!(input(node(&b, None, "SR"), "base_color").port_type, PortType::Value(Type::Color3));
}

#[test]
fn colorspace_is_kept_on_inputs_and_interface_inputs() {
    let b = twin();
    assert_eq!(input(node(&b, Some("NG"), "img"), "file").colorspace.as_deref(), Some("srgb_texture"));
    let (mut b, ng, mul, _, _) = small();
    b.add_interface_input(ng, "tex", "filename", Some(V::Text("a.png")), None).unwrap();
    b.set_interface_colorspace(ng, "tex", "srgb_texture").unwrap();
    assert_eq!(b.document().nodegraphs[0].interface[1].colorspace.as_deref(), Some("srgb_texture"));
    // A colorspace on a type without one is dropped, as the XML reader drops it.
    b.set_interface_colorspace(ng, "k", "srgb_texture").unwrap();
    assert_eq!(b.document().nodegraphs[0].interface[0].colorspace, None);
    // It may follow a connection too.
    b.connect(mul, "in2", Some("color3"), Source::Interface("k"), None).unwrap();
    b.set_input_colorspace(mul, "in2", "acescg").unwrap();
    assert_eq!(input(node(&b, Some("NG"), "mul"), "in2").colorspace.as_deref(), Some("acescg"));
    let err = b.set_input_colorspace(mul, "in9", "acescg").unwrap_err();
    says(err, &["in9"]);
}

#[test]
fn duplicate_names_are_errors_naming_the_source_path() {
    let (mut b, ng, mul, _, _) = small();
    says(b.add_node(Some(ng), "add", "float", "mul", Some("/NG/mul2")).unwrap_err(), &["/NG/mul2", "mul"]);
    says(b.add_nodegraph("NG", Some("/NG2")).unwrap_err(), &["/NG2", "NG"]);
    says(b.add_node(None, "add", "float", "NG", Some("/NG3")).unwrap_err(), &["/NG3", "NG"]);
    says(b.add_interface_input(ng, "k", "float", None, Some("/NG.inputs:k")).unwrap_err(), &["/NG.inputs:k"]);
    says(b.add_node(Some(ng), "add", "float", "k", Some("/NG/k")).unwrap_err(), &["/NG/k"]);
    b.set_input(mul, "in1", Some("color3"), V::Floats(&[1.0, 1.0, 1.0]), None).unwrap();
    let err = b.connect(mul, "in1", Some("color3"), Source::Interface("k"), Some("/NG/mul.inputs:in1"));
    says(err.unwrap_err(), &["/NG/mul.inputs:in1", "in1"]);
    b.add_output(Some(ng), "out", "color3", Source::Node(mul, None), None).unwrap();
    let err = b.add_output(Some(ng), "out", "color3", Source::Node(mul, None), Some("/NG.outputs:out"));
    says(err.unwrap_err(), &["/NG.outputs:out"]);
    // The same name in another scope is fine.
    b.add_node(None, "add", "float", "mul", None).unwrap();
}

#[test]
#[ignore = "Task 3"]
fn duplicate_names_in_xml_are_errors_naming_the_line() {
    let text = "<materialx version=\"1.39\">\n<nodegraph name=\"g\">\n<add name=\"a\" type=\"float\"/>\n\
                <add name=\"a\" type=\"float\"/>\n</nodegraph></materialx>";
    let err = build(&parse_str(text, None).unwrap(), Some("dup.mtlx")).unwrap_err();
    says(err, &["dup.mtlx:4", "g/a"]);
}

#[test]
fn a_missing_graph_output_fails_at_to_ir_naming_the_source_path() {
    let (mut b, _, _, _, sr) = small();
    b.connect(sr, "base_color", None, Source::Graph(GraphRef::default(), "nope"), Some("/SR.inputs:base_color"))
        .unwrap();
    b.add_material("M", sr, None).unwrap();
    let err = b.to_ir(None, &IrOptions::default()).unwrap_err();
    says(err, &["/SR.inputs:base_color", "nope"]);
}

#[test]
fn an_interface_source_must_be_declared_in_the_enclosing_nodegraph() {
    let (mut b, ng, mul, _, sr) = small();
    let err = b.connect(mul, "in1", Some("color3"), Source::Interface("ghost"), Some("/NG/mul.inputs:in1"));
    says(err.unwrap_err(), &["/NG/mul.inputs:in1", "ghost"]);
    let err = b.add_output(Some(ng), "o", "float", Source::Interface("ghost"), Some("/NG.outputs:o"));
    says(err.unwrap_err(), &["/NG.outputs:o", "ghost"]);
    let err = b.connect(sr, "base", None, Source::Interface("k"), Some("/SR.inputs:base"));
    says(err.unwrap_err(), &["/SR.inputs:base"]);
}

#[test]
fn a_node_of_another_scope_is_not_a_source() {
    let (mut b, _, mul, _, sr) = small();
    let err = b.connect(sr, "base_color", None, Source::Node(mul, None), Some("/SR.inputs:base_color"));
    says(err.unwrap_err(), &["/SR.inputs:base_color", "mul"]);
    let err = b.connect(mul, "in1", Some("color3"), Source::Node(sr, None), Some("/NG/mul.inputs:in1"));
    says(err.unwrap_err(), &["/NG/mul.inputs:in1", "SR"]);
}

#[test]
fn a_material_needs_a_document_level_shader() {
    let (mut b, _, mul, _, _) = small();
    says(b.add_material("M", mul, Some("/M")).unwrap_err(), &["/M", "mul"]);
}

#[test]
fn an_elaboration_error_names_the_source_path() {
    let mut b = DocumentBuilder::new();
    let bogus = b.add_node(None, "bogus", "color3", "b", Some("/root/b")).unwrap();
    let sr = b.add_node_from_nodedef(None, "ND_standard_surface_surfaceshader", "SR", None).unwrap();
    b.connect(sr, "base_color", None, Source::Node(bogus, None), None).unwrap();
    b.add_material("M", sr, None).unwrap();
    let err = b.to_ir(None, &IrOptions::default()).unwrap_err();
    assert!(matches!(err, Error::UnknownNode { .. }), "{err}");
    says(err, &["/root/b (b)", "bogus"]);
}

#[test]
fn handles_of_another_builder_are_refused() {
    let (_, ng, mul, _, _) = small();
    let mut other = DocumentBuilder::new();
    assert!(other.set_input(mul, "in1", Some("color3"), V::Floats(&[1.0, 1.0, 1.0]), None).is_err());
    assert!(other.add_interface_input(ng, "x", "float", None, None).is_err());
}

#[test]
fn values_round_trip_to_the_f32_their_text_gives() {
    let values = [0.1f32, 1e-7, -0.0, 3.4e38];
    let inputs = ["base", "specular", "metalness", "coat"];
    let mut b = DocumentBuilder::new();
    let sr = b.add_node_from_nodedef(None, "ND_standard_surface_surfaceshader", "SR", None).unwrap();
    for (i, (value, input)) in values.iter().zip(inputs).enumerate() {
        let c = b.add_node_from_nodedef(None, "ND_constant_float", &format!("c{i}"), None).unwrap();
        b.set_input(c, "value", None, V::Floats(&[*value]), None).unwrap();
        b.connect(sr, input, None, Source::Node(c, None), None).unwrap();
    }
    b.add_material("M", sr, None).unwrap();
    let xml = r#"<materialx version="1.39">
      <standard_surface name="SR" type="surfaceshader" nodedef="ND_standard_surface_surfaceshader">
        <input name="base" type="float" nodename="c0"/>
        <input name="specular" type="float" nodename="c1"/>
        <input name="metalness" type="float" nodename="c2"/>
        <input name="coat" type="float" nodename="c3"/>
      </standard_surface>
      <constant name="c0" type="float" nodedef="ND_constant_float"><input name="value" type="float" value="0.1"/></constant>
      <constant name="c1" type="float" nodedef="ND_constant_float"><input name="value" type="float" value="1e-7"/></constant>
      <constant name="c2" type="float" nodedef="ND_constant_float"><input name="value" type="float" value="-0.0"/></constant>
      <constant name="c3" type="float" nodedef="ND_constant_float"><input name="value" type="float" value="3.4e38"/></constant>
      <surfacematerial name="M" type="material"><input name="surfaceshader" type="surfaceshader" nodename="SR"/></surfacematerial>
    </materialx>"#;
    for options in [IrOptions::default(), IrOptions { values_as_uniforms: false, ..IrOptions::default() }] {
        assert_eq!(export(&b.to_ir(None, &options).unwrap().dag), xml_ir(xml, &options), "{options:?}");
    }
}

#[test]
fn non_finite_values_are_refused() {
    let (mut b, ng, _, _, sr) = small();
    let err = b.set_input(sr, "base", None, V::Floats(&[f32::NAN]), Some("/SR.inputs:base")).unwrap_err();
    says(err, &["/SR.inputs:base"]);
    let err = b.set_input(sr, "coat", None, V::Floats(&[f32::INFINITY]), Some("/SR.inputs:coat")).unwrap_err();
    says(err, &["/SR.inputs:coat"]);
    let err =
        b.add_interface_input(ng, "v", "vector2", Some(V::Floats(&[1.0, f32::NEG_INFINITY])), Some("/NG.inputs:v"));
    says(err.unwrap_err(), &["/NG.inputs:v"]);
}

#[test]
fn paths_with_spaces_and_unicode_survive() {
    const PATH: &str = "D:/Downloads/SeaArcher_UE (1)/tex/\u{e4}.png";
    let mut b = DocumentBuilder::new();
    let img = b.add_node_from_nodedef(None, "ND_image_color3", "img", None).unwrap();
    b.set_input(img, "file", None, V::Text(PATH), None).unwrap();
    let sr = b.add_node_from_nodedef(None, "ND_standard_surface_surfaceshader", "SR", None).unwrap();
    b.connect(sr, "base_color", None, Source::Node(img, None), None).unwrap();
    b.add_material("M", sr, None).unwrap();
    let ir = b.to_ir(None, &IrOptions::default()).unwrap();
    let files: Vec<&str> = ir.dag.textures.iter().map(|t| t.file.as_str()).collect();
    assert_eq!(files, [PATH]);
}

#[test]
fn empty_names_are_refused() {
    let (mut b, ng, _, _, _) = small();
    says(b.add_node(Some(ng), "add", "float", "", Some("/NG/a")).unwrap_err(), &["/NG/a"]);
    says(b.add_node_from_nodedef(None, "ND_add_float", "", Some("/b")).unwrap_err(), &["/b"]);
    says(b.add_nodegraph("", Some("/G")).unwrap_err(), &["/G"]);
    says(b.add_interface_input(ng, "", "float", None, Some("/NG.inputs:")).unwrap_err(), &["/NG.inputs:"]);
}

#[test]
fn to_ir_can_run_twice_and_after_an_edit() {
    let mut b = twin();
    let sr2 = b.add_node_from_nodedef(None, "ND_standard_surface_surfaceshader", "SR2", None).unwrap();
    b.add_material("M2", sr2, None).unwrap();
    let options = IrOptions::default();
    let first = export(&b.to_ir(Some("M"), &options).unwrap().dag);
    assert_eq!(first, export(&b.to_ir(Some("M"), &options).unwrap().dag));
    assert_eq!(first, xml_ir(TWIN_XML, &options));
    let before = export(&b.to_ir(Some("M2"), &options).unwrap().dag);
    b.set_input(sr2, "base_color", None, V::Floats(&[0.25, 0.5, 0.75]), None).unwrap();
    let after = export(&b.to_ir(Some("M2"), &options).unwrap().dag);
    assert_ne!(before, after);
    let edited = r#"<materialx version="1.39">
      <standard_surface name="SR2" type="surfaceshader" nodedef="ND_standard_surface_surfaceshader">
        <input name="base_color" type="color3" value="0.25, 0.5, 0.75"/>
      </standard_surface>
      <surfacematerial name="M2" type="material"><input name="surfaceshader" type="surfaceshader" nodename="SR2"/></surfacematerial>
    </materialx>"#;
    assert_eq!(after, xml_ir(edited, &options));
}

#[test]
fn a_connection_cycle_fails_naming_a_source_path() {
    let mut b = DocumentBuilder::new();
    let ng = b.add_nodegraph("NG", None).unwrap();
    let a = b.add_node(Some(ng), "add", "float", "a", Some("/NG/a")).unwrap();
    let c = b.add_node(Some(ng), "add", "float", "c", Some("/NG/c")).unwrap();
    b.connect(a, "in1", Some("float"), Source::Node(c, None), None).unwrap();
    b.connect(c, "in1", Some("float"), Source::Node(a, None), None).unwrap();
    b.add_output(Some(ng), "o", "float", Source::Node(a, None), None).unwrap();
    let sr = b.add_node_from_nodedef(None, "ND_standard_surface_surfaceshader", "SR", None).unwrap();
    b.connect(sr, "base", None, Source::Graph(ng, "o"), None).unwrap();
    b.add_material("M", sr, None).unwrap();
    let text = b.to_ir(None, &IrOptions::default()).unwrap_err().to_string();
    assert!(text.contains("/NG/a") || text.contains("/NG/c"), "{text}");
}

#[test]
fn a_default_graph_handle_is_refused_outside_a_connection_source() {
    let (mut b, _, _, _, _) = small();
    let none = GraphRef::default();
    assert!(b.add_node(Some(none), "add", "float", "a", None).is_err());
    assert!(b.add_node_from_nodedef(Some(none), "ND_add_float", "a", None).is_err());
    assert!(b.add_interface_input(none, "x", "float", None, None).is_err());
    assert!(b.set_interface_colorspace(none, "k", "srgb_texture").is_err());
    let mul_source = Source::Interface("k");
    assert!(b.add_output(Some(none), "o", "float", mul_source, None).is_err());
    // Nothing landed in the first nodegraph.
    let graph = &b.document().nodegraphs[0];
    assert_eq!((graph.interface.len(), graph.nodes.len(), graph.outputs.len()), (1, 2, 0));
}
