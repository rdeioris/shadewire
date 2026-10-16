//! Colorspace and unit conversion of authored values, and which values become uniforms.

use shadewire::interpret::{ContextSource, NoTextures, eval_surface};
use shadewire::optimize::optimize;
use shadewire::{ContextRead, Dag, Error, NodeKind, Op, Space, TransformKind, Value};

use super::tests::doc;
use super::{elaborate_output, elaborate_output_with};
use crate::document::Document;
use crate::options::IrOptions;

struct Zeros;
impl ContextSource for Zeros {
    fn read(&mut self, read: &ContextRead) -> Value {
        Value::zero(read.ty())
    }
    fn transform(&mut self, _: TransformKind, _: Space, _: Space, v: [f32; 3]) -> [f32; 3] {
        v
    }
}

fn eval(dag: &Dag) -> Value {
    eval_surface(dag, &mut Zeros, &NoTextures).remove(0).1
}

/// Graph `g` with one `constant` of type `ty` whose value is authored in `colorspace`.
fn constant_in(ty: &str, value: &str, colorspace: &str) -> Document {
    doc(&format!(
        r#"<nodegraph name="g">
          <constant name="c" type="{ty}"><input name="value" type="{ty}" value="{value}" colorspace="{colorspace}"/></constant>
          <output name="out" type="{ty}" nodename="c"/>
        </nodegraph>"#
    ))
}

fn assert_close(got: &[f32], want: &[f32]) {
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < 1e-4, "got {got:?}, want {want:?}");
    }
}

#[test]
fn srgb_texture_constant_folds_to_linear() {
    let dag = elaborate_output(&constant_in("color3", "0.5,0.5,0.5", "srgb_texture"), "g", "out").unwrap();
    let Value::Color3(c) = eval(&dag) else { panic!("not a color3") };
    assert_close(&c, &[0.2140, 0.2140, 0.2140]);
    let folded = optimize(&dag);
    let NodeKind::Const(Value::Color3(c)) = &folded.node(folded.surface.fields[0].node).kind else {
        panic!("not folded")
    };
    assert_close(c, &[0.2140, 0.2140, 0.2140]);
}

#[test]
fn srgb_texture_color4_keeps_alpha() {
    let dag = elaborate_output(&constant_in("color4", "0.5,0.5,0.5,0.5", "srgb_texture"), "g", "out").unwrap();
    let Value::Color4(c) = eval(&dag) else { panic!("not a color4") };
    assert_close(&c, &[0.2140, 0.2140, 0.2140, 0.5]);
}

#[test]
fn working_colorspace_none_and_raw_are_unchanged() {
    for colorspace in ["lin_rec709", "none", "raw"] {
        let dag = elaborate_output(&constant_in("color3", "0.5,0.5,0.5", colorspace), "g", "out").unwrap();
        assert_eq!(
            dag.node(dag.surface.fields[0].node).kind,
            NodeKind::Const(Value::Color3([0.5, 0.5, 0.5])),
            "{colorspace}"
        );
    }
}

#[test]
fn acescg_red_goes_through_the_cmlib_matrix() {
    let dag = elaborate_output(&constant_in("color3", "1,0,0", "acescg"), "g", "out").unwrap();
    let Value::Color3(c) = eval(&dag) else { panic!("not a color3") };
    assert_close(&c, &[1.705051, -0.130256, -0.024003]);
}

#[test]
fn unknown_colorspace_is_unsupported() {
    let err = elaborate_output(&constant_in("color3", "1,0,0", "martian"), "g", "out").unwrap_err();
    let Error::UnsupportedColorspace { colorspace, .. } = err else { panic!("{err:?}") };
    assert_eq!(colorspace, "martian");
}

/// The IR of a color3 constant `0.8,0.4,0.2` authored in `colorspace`, as text.
fn constant_ir(colorspace: &str) -> String {
    elaborate_output(&constant_in("color3", "0.8,0.4,0.2", colorspace), "g", "out")
        .unwrap_or_else(|e| panic!("{colorspace}: {e}"))
        .dump()
}

fn assert_same_ir_as(canonical: &str, aliases: &[&str]) {
    let want = constant_ir(canonical);
    for alias in aliases {
        assert_eq!(constant_ir(alias), want, "{alias} should read as {canonical}");
    }
}

#[test]
fn linear_rec709_aliases_are_the_working_colorspace() {
    let none = constant_ir("none");
    assert_eq!(constant_ir("lin_rec709"), none);
    assert_same_ir_as(
        "lin_rec709",
        &[
            "lin_srgb",
            "lin_rec709_scene",
            "Linear Rec.709 (sRGB)",
            "Utility - Linear - sRGB",
            "Utility - Linear - Rec.709",
        ],
    );
}

#[test]
fn srgb_texture_aliases_convert_like_srgb_texture() {
    assert_ne!(constant_ir("srgb_texture"), constant_ir("none"));
    assert_same_ir_as(
        "srgb_texture",
        &["srgb_rec709_scene", "srgb_tx", "sRGB - Texture", "Utility - sRGB - Texture", "sRGB Encoded Rec.709 (sRGB)"],
    );
}

#[test]
fn acescg_aliases_convert_like_acescg() {
    assert_ne!(constant_ir("acescg"), constant_ir("none"));
    assert_same_ir_as("acescg", &["ACEScg", "ACES - ACEScg", "lin_ap1", "lin_ap1_scene"]);
}

#[test]
fn usd_scene_names_convert_like_their_cmlib_spaces() {
    assert_same_ir_as("g22_rec709", &["g22_rec709_scene"]);
    assert_same_ir_as("g18_rec709", &["g18_rec709_scene", "gamma18"]);
    assert_same_ir_as("g22_ap1", &["g22_ap1_scene"]);
    assert_same_ir_as("lin_displayp3", &["lin_p3d65_scene"]);
    assert_same_ir_as("srgb_displayp3", &["srgb_p3d65_scene"]);
    assert_same_ir_as("lin_adobergb", &["lin_adobergb_scene"]);
    assert_same_ir_as("adobergb", &["g22_adobergb_scene"]);
}

#[test]
fn every_alias_converts_like_its_cmlib_colorspace() {
    for (alias, canonical) in super::colorspace::ALIASES {
        assert_same_ir_as(canonical, &[alias]);
    }
}

#[test]
fn not_color_names_behave_as_raw() {
    assert_same_ir_as("raw", &["data", "identity", "Raw", "Utility - Raw"]);
    assert_eq!(constant_ir("data"), constant_ir("none"));
}

#[test]
fn unknown_colorspace_error_names_the_authored_string() {
    for authored in ["martian", "LIN_SRGB", "srgb_ap1_scene", "Linear Rec.709"] {
        let err = elaborate_output(&constant_in("color3", "1,0,0", authored), "g", "out").unwrap_err();
        let Error::UnsupportedColorspace { colorspace, .. } = err else { panic!("{err:?}") };
        assert_eq!(colorspace, authored);
    }
}

#[test]
fn an_aliased_working_colorspace_is_canonicalised() {
    let options = IrOptions { working_colorspace: "lin_srgb".to_string(), ..IrOptions::default() };
    let ir = |colorspace: &str| {
        elaborate_output_with(&constant_in("color3", "0.8,0.4,0.2", colorspace), "g", "out", &options)
            .unwrap_or_else(|e| panic!("{colorspace}: {e}"))
            .dump()
    };
    assert_eq!(ir("lin_rec709"), constant_ir("none"));
    assert_eq!(ir("srgb_texture"), constant_ir("srgb_texture"));
}

#[test]
fn color_images_convert_their_samples_and_float_images_do_not() {
    let d = doc(r#"<nodegraph name="g">
          <image name="c" type="color3"><input name="file" type="filename" value="a.png" colorspace="srgb_texture"/></image>
          <image name="f" type="float"><input name="file" type="filename" value="b.png" colorspace="srgb_texture"/></image>
          <output name="out" type="color3" nodename="c"/>
          <output name="rough" type="float" nodename="f"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    assert!(
        !matches!(dag.node(dag.surface.fields[0].node).kind, NodeKind::Sample { .. }),
        "color sample not converted"
    );
    let dag = elaborate_output(&d, "g", "rough").unwrap();
    assert!(matches!(dag.node(dag.surface.fields[0].node).kind, NodeKind::Sample { .. }));
}

/// A nodedef whose `len` input is in meters, implemented as a pass-through.
const METERS: &str = r#"<nodedef name="ND_len_float" node="len">
      <input name="len" type="float" value="0" unittype="distance" unit="meter"/>
      <input name="turn" type="float" value="0" unittype="angle" unit="degree"/>
      <output name="out" type="float"/>
    </nodedef>
    <nodegraph name="NG_len_float" nodedef="ND_len_float">
      <add name="a" type="float">
        <input name="in1" type="float" interfacename="len"/>
        <input name="in2" type="float" interfacename="turn"/>
      </add>
      <output name="out" type="float" nodename="a"/>
    </nodegraph>"#;

#[test]
fn centimeters_become_meters() {
    let d = doc(&format!(
        r#"{METERS}<nodegraph name="g">
          <len name="l" type="float"><input name="len" type="float" value="100" unit="centimeter" unittype="distance"/></len>
          <output name="out" type="float" nodename="l"/>
        </nodegraph>"#
    ));
    let dag = elaborate_output(&d, "g", "out").unwrap();
    let Value::Float(v) = eval(&dag) else { panic!() };
    assert!((v - 1.0).abs() < 1e-6, "{v}");
    assert!(dag.approximations.is_empty(), "{:?}", dag.approximations);
}

#[test]
fn other_unit_types_pass_through_as_approximations() {
    let d = doc(&format!(
        r#"{METERS}<nodegraph name="g">
          <len name="l" type="float"><input name="turn" type="float" value="3" unit="radian" unittype="angle"/></len>
          <output name="out" type="float" nodename="l"/>
        </nodegraph>"#
    ));
    let dag = elaborate_output(&d, "g", "out").unwrap();
    assert_eq!(eval(&dag), Value::Float(3.0));
    assert_eq!(dag.approximations, ["unit angle ignored".to_string()]);
}

fn uniform_names(dag: &Dag) -> Vec<String> {
    let mut names: Vec<String> = dag
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Uniform { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    names.sort();
    names
}

#[test]
fn top_level_graph_interface_values_are_uniquely_named_uniforms() {
    let d = doc(r#"<nodegraph name="a">
          <input name="scale" type="float" value="2"/>
          <dot name="d" type="float"><input name="in" type="float" interfacename="scale"/></dot>
          <output name="o" type="float" nodename="d"/>
        </nodegraph>
        <nodegraph name="b">
          <input name="scale" type="float" value="3"/>
          <dot name="d" type="float"><input name="in" type="float" interfacename="scale"/></dot>
          <output name="o" type="float" nodename="d"/>
        </nodegraph>
        <nodegraph name="g">
          <add name="s" type="float">
            <input name="in1" type="float" nodegraph="a" output="o"/>
            <input name="in2" type="float" nodegraph="b" output="o"/>
          </add>
          <output name="out" type="float" nodename="s"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    assert_eq!(uniform_names(&dag), ["scale", "scale_2"]);
    assert_eq!(eval(&dag), Value::Float(5.0));
}

#[test]
fn node_values_inside_graphs_stay_constants() {
    let dag = elaborate_output(&constant_in("float", "4", "none"), "g", "out").unwrap();
    assert!(uniform_names(&dag).is_empty());
}

#[test]
fn op_params_never_become_uniforms() {
    let d = doc(r#"<nodegraph name="g">
          <input name="idx" type="integer" value="1"/>
          <extract name="e" type="float">
            <input name="in" type="vector3" value="1,2,3"/>
            <input name="index" type="integer" interfacename="idx"/>
          </extract>
          <extract name="f" type="float">
            <input name="in" type="vector3" value="1,2,3"/>
            <input name="index" type="integer" value="2"/>
          </extract>
          <texcoord name="t" type="vector2"><input name="index" type="integer" interfacename="idx"/></texcoord>
          <output name="out" type="float" nodename="e"/>
          <output name="last" type="float" nodename="f"/>
          <output name="uv" type="vector2" nodename="t"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    let NodeKind::Op { op, .. } = &dag.node(dag.surface.fields[0].node).kind else { panic!("not an op") };
    assert_eq!(*op, Op::Extract(1));
    let dag = elaborate_output(&d, "g", "last").unwrap();
    let NodeKind::Op { op, .. } = &dag.node(dag.surface.fields[0].node).kind else { panic!("not an op") };
    assert_eq!(*op, Op::Extract(2));
    let dag = elaborate_output(&d, "g", "uv").unwrap();
    let read = &dag.node(dag.surface.fields[0].node).kind;
    assert_eq!(*read, NodeKind::Context(ContextRead::TexCoord { index: 1, ty: shadewire::Type::Vector2 }));
}
