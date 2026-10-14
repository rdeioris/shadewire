use std::path::Path;

use shadewire::interpret::{ContextSource, NoTextures, eval_surface};
use shadewire::{ContextRead, Dag, Error, NodeKind, Op, Space, TransformKind, Type, Value};

use super::models::{KNOWN_MODELS, fields};
use crate::document::Document;
use crate::document::build::build;
use crate::index::Index;
use crate::library::standard;
use crate::parse::parse_str;
use crate::{ElaborateOptions, elaborate};

const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/materialx/resources/Materials/Examples");

fn load(relative: &str) -> Document {
    let path = Path::new(EXAMPLES).join(relative);
    let text = std::fs::read_to_string(&path).unwrap();
    let file = path.to_string_lossy().to_string();
    build(&parse_str(&text, Some(&file)).unwrap(), Some(&file)).unwrap()
}

fn inline(body: &str) -> Document {
    let text = format!(r#"<materialx version="1.39">{body}</materialx>"#);
    build(&parse_str(&text, None).unwrap(), Some("test.mtlx")).unwrap()
}

fn compile(doc: &Document, material: Option<&str>, models: &[&str]) -> Result<Dag, Error> {
    elaborate(doc, standard(), &ElaborateOptions::default(), material, models)
}

fn field_count(nodedef: &str) -> usize {
    let empty = Document::empty();
    let index = Index::new(&empty, standard());
    fields(index.nodedef(nodedef).unwrap()).len()
}

struct Zeros;
impl ContextSource for Zeros {
    fn read(&mut self, read: &ContextRead) -> Value {
        Value::zero(read.ty())
    }
    fn transform(&mut self, _: TransformKind, _: Space, _: Space, v: [f32; 3]) -> [f32; 3] {
        v
    }
}

fn field_value(dag: &Dag, name: &str) -> Value {
    let values = eval_surface(dag, &mut Zeros, &NoTextures);
    values.into_iter().find(|(n, _)| n == name).unwrap().1
}

#[test]
fn open_pbr_default_has_every_field() {
    let doc = load("OpenPbr/open_pbr_default.mtlx");
    let dag = compile(&doc, None, &KNOWN_MODELS).unwrap();
    assert_eq!(dag.surface.model, "open_pbr_surface");
    assert_eq!(dag.surface.fields.len(), field_count("ND_open_pbr_surface_surfaceshader"));
    assert_eq!(dag.surface.fields[1].name, "base_color");
    assert_eq!(dag.surface.fields[1].ty, Type::Color3);
    let base_color = dag.node(dag.surface.fields[1].node);
    assert!(matches!(&base_color.kind, NodeKind::Uniform { name, .. } if name == "base_color"), "{base_color:?}");
    assert_eq!(field_value(&dag, "specular_roughness"), Value::Float(0.3));
    assert!(dag.approximations.is_empty());
}

#[test]
fn gltf_pbr_default_compiles() {
    let doc = load("GltfPbr/gltf_pbr_default.mtlx");
    let dag = compile(&doc, None, &KNOWN_MODELS).unwrap();
    assert_eq!(dag.surface.model, "gltf_pbr");
    assert_eq!(dag.surface.fields.len(), field_count("ND_gltf_pbr_surfaceshader"));
}

#[test]
fn unsupported_host_model_is_no_translation() {
    let doc = load("StandardSurface/standard_surface_default.mtlx");
    let err = compile(&doc, None, &["open_pbr_surface"]).unwrap_err();
    assert_eq!(
        err,
        Error::NoTranslation { from: "standard_surface".to_string(), to: vec!["open_pbr_surface".to_string()] }
    );
}

const TWO_MATERIALS: &str = r#"
    <standard_surface name="s1" type="surfaceshader"><input name="base" type="float" value="0.5"/></standard_surface>
    <surfacematerial name="first" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s1"/></surfacematerial>
    <surfacematerial name="second" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s1"/></surfacematerial>"#;

#[test]
fn several_materials_need_a_name() {
    let doc = inline(TWO_MATERIALS);
    let err = compile(&doc, None, &KNOWN_MODELS).unwrap_err();
    assert_eq!(err, Error::AmbiguousMaterial { names: vec!["first".to_string(), "second".to_string()] });
    assert_eq!(compile(&doc, Some("second"), &KNOWN_MODELS).unwrap().surface.model, "standard_surface");
    let err = compile(&doc, Some("third"), &KNOWN_MODELS).unwrap_err();
    assert_eq!(err, Error::UnknownMaterial { name: "third".to_string() });
}

#[test]
fn no_material_is_an_error() {
    let doc = inline(r#"<standard_surface name="s1" type="surfaceshader"/>"#);
    assert_eq!(compile(&doc, None, &KNOWN_MODELS).unwrap_err(), Error::NoMaterial);
}

const MIX: &str = r#"
    <standard_surface name="red" type="surfaceshader">
      <input name="base_color" type="color3" value="1, 0, 0"/>
      <input name="thin_walled" type="boolean" value="true"/>
    </standard_surface>
    <standard_surface name="blue" type="surfaceshader">
      <input name="base_color" type="color3" value="0, 0, 1"/>
    </standard_surface>
    <mix name="blend" type="surfaceshader">
      <input name="fg" type="surfaceshader" nodename="red"/>
      <input name="bg" type="surfaceshader" nodename="blue"/>
      <input name="mix" type="float" value="0.25"/>
    </mix>
    <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="blend"/></surfacematerial>"#;

#[test]
fn same_model_mix_lerps_and_is_flagged() {
    let dag = compile(&inline(MIX), None, &KNOWN_MODELS).unwrap();
    assert_eq!(dag.surface.model, "standard_surface");
    assert_eq!(dag.surface.fields.len(), field_count("ND_standard_surface_surfaceshader"));
    assert_eq!(dag.approximations, ["surface_mix"]);
    let base_color = dag.surface.fields.iter().find(|f| f.name == "base_color").unwrap();
    assert!(matches!(&dag.node(base_color.node).kind, NodeKind::Op { op: Op::Mix, .. }));
    assert_eq!(field_value(&dag, "base_color"), Value::Color3([0.25, 0.0, 0.75]));
    // Booleans cannot blend: the side with the larger weight wins.
    assert_eq!(field_value(&dag, "thin_walled"), Value::Boolean(false));
}

#[test]
fn mix_of_different_models_is_unsupported() {
    let blue = r#"<standard_surface name="blue" type="surfaceshader">
      <input name="base_color" type="color3" value="0, 0, 1"/>
    </standard_surface>"#;
    assert!(MIX.contains(blue));
    let doc = inline(&MIX.replace(blue, r#"<open_pbr_surface name="blue" type="surfaceshader"/>"#));
    let err = compile(&doc, None, &KNOWN_MODELS).unwrap_err();
    assert!(matches!(&err, Error::UnsupportedSurface { reason, .. } if reason == "mix of different models"), "{err:?}");
}

#[test]
fn follows_dot_and_user_nodegraph_outputs() {
    let doc = inline(
        r#"
        <nodegraph name="shading">
          <gltf_pbr name="inner" type="surfaceshader"><input name="metallic" type="float" value="0.5"/></gltf_pbr>
          <output name="out" type="surfaceshader" nodename="inner"/>
        </nodegraph>
        <dot name="pass" type="surfaceshader"><input name="in" type="surfaceshader" nodegraph="shading" output="out"/></dot>
        <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="pass"/></surfacematerial>"#,
    );
    let dag = compile(&doc, None, &KNOWN_MODELS).unwrap();
    assert_eq!(dag.surface.model, "gltf_pbr");
    assert_eq!(field_value(&dag, "metallic"), Value::Float(0.5));
}

#[test]
fn follows_a_nodegraph_implemented_shader_nodedef() {
    let doc = inline(
        r#"
        <nodedef name="ND_shiny" node="shiny">
          <input name="tint" type="color3" value="1, 1, 1"/>
          <output name="out" type="surfaceshader"/>
        </nodedef>
        <nodegraph name="NG_shiny" nodedef="ND_shiny">
          <standard_surface name="s" type="surfaceshader">
            <input name="base_color" type="color3" interfacename="tint"/>
            <input name="metalness" type="float" value="1"/>
          </standard_surface>
          <output name="out" type="surfaceshader" nodename="s"/>
        </nodegraph>
        <shiny name="gold" type="surfaceshader"><input name="tint" type="color3" value="1, 0.8, 0.2"/></shiny>
        <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="gold"/></surfacematerial>"#,
    );
    let dag = compile(&doc, None, &KNOWN_MODELS).unwrap();
    assert_eq!(dag.surface.model, "standard_surface");
    assert_eq!(field_value(&dag, "base_color"), Value::Color3([1.0, 0.8, 0.2]));
    let metalness = dag.surface.fields.iter().find(|f| f.name == "metalness").unwrap();
    assert_eq!(dag.node(metalness.node).kind, NodeKind::Const(Value::Float(1.0)));
}

#[test]
fn other_shaders_are_not_a_known_shading_model() {
    let doc = inline(
        r#"
        <oren_nayar_diffuse_bsdf name="bsdf" type="BSDF"/>
        <surface name="surf" type="surfaceshader"><input name="bsdf" type="BSDF" nodename="bsdf"/></surface>
        <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="surf"/></surfacematerial>"#,
    );
    let err = compile(&doc, None, &KNOWN_MODELS).unwrap_err();
    let known = matches!(&err, Error::UnsupportedSurface { node, reason, .. } if node == "surf" && reason == "not a known shading model");
    assert!(known, "{err:?}");
}

#[test]
fn standard_surface_versions_read_their_own_defaults() {
    let material = r#"<surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>"#;
    let current = inline(&format!(r#"<standard_surface name="s" type="surfaceshader"/>{material}"#));
    assert_eq!(field_value(&compile(&current, None, &KNOWN_MODELS).unwrap(), "base"), Value::Float(1.0));
    let old = inline(&format!(r#"<standard_surface name="s" type="surfaceshader" version="1.0.0"/>{material}"#));
    assert_eq!(field_value(&compile(&old, None, &KNOWN_MODELS).unwrap(), "base"), Value::Float(0.8));
}

#[test]
fn geomprop_defaults_reach_the_dag() {
    let doc = inline(
        r#"
        <geompropvalue name="tint" type="color3">
          <input name="geomprop" type="string" value="tintprop"/>
          <input name="default" type="color3" value="1, 1, 1"/>
        </geompropvalue>
        <standard_surface name="s" type="surfaceshader">
          <input name="base_color" type="color3" nodename="tint"/>
        </standard_surface>
        <surfacematerial name="m" type="material">
          <input name="surfaceshader" type="surfaceshader" nodename="s"/>
        </surfacematerial>"#,
    );
    let dag = compile(&doc, None, &KNOWN_MODELS).unwrap();
    assert_eq!(dag.geomprop_defaults.get("tintprop"), Some(&Value::Color3([1.0, 1.0, 1.0])));
}

#[test]
fn ui_attributes_reach_the_dag_from_the_input_or_its_nodedef_port() {
    let doc = inline(
        r#"
        <standard_surface name="s" type="surfaceshader">
          <input name="base" type="float" value="0.5" uisoftmin="0.2" uimax="0.9" uifolder="Mine/Sub"/>
          <input name="base_color" type="color3" value="1, 0, 0"/>
          <input name="specular" type="float" value="1" uimin="low"/>
        </standard_surface>
        <surfacematerial name="m" type="material">
          <input name="surfaceshader" type="surfaceshader" nodename="s"/>
        </surfacematerial>"#,
    );
    let dag = compile(&doc, None, &KNOWN_MODELS).unwrap();
    // The input's own attributes win; the rest come from ND_standard_surface_surfaceshader.
    let base = &dag.uniform_ui["base"];
    assert_eq!(base.min, Some(Value::Float(0.0)));
    assert_eq!(base.max, Some(Value::Float(0.9)));
    assert_eq!(base.soft_min, Some(Value::Float(0.2)));
    assert_eq!(base.soft_max, None);
    assert_eq!(base.folder.as_deref(), Some("Mine/Sub"));
    let color = &dag.uniform_ui["base_color"];
    assert_eq!(color.max, Some(Value::Color3([1.0, 1.0, 1.0])));
    assert_eq!(color.folder.as_deref(), Some("Base"));
    // A limit that is not a float is dropped; the nodedef's limit stays out too, as the input authored one.
    assert_eq!(dag.uniform_ui["specular"].min, None);
}

#[test]
fn ui_attributes_of_top_level_interface_inputs() {
    let doc = load("StandardSurface/standard_surface_marble_solid.mtlx");
    let dag = compile(&doc, None, &KNOWN_MODELS).unwrap();
    let octaves = &dag.uniform_ui["noise_octaves"];
    assert_eq!(
        (octaves.soft_min.clone(), octaves.soft_max.clone()),
        (Some(Value::Integer(1)), Some(Value::Integer(8)))
    );
    assert_eq!(octaves.folder.as_deref(), Some("Marble Noise"));
}
