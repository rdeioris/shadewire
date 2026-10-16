//! The writer: MaterialX and MDL materials compiled to MDL modules, checked by what they contain and, when it is
//! installed, by `mdlc`.

mod common;

use common::assert_mdlc;
use shadewire::{Error, LOBE_MODELS};
use shadewire_mdl::{IrOptions, MdlOptions, compile};

fn mtlx(text: &str, models: &[&str]) -> shadewire_nodes::MaterialIr {
    let options = IrOptions::with_models(models);
    shadewire_mtlx::to_ir(text, None, None, &options, &shadewire_mtlx::Resolver::default())
        .unwrap_or_else(|e| panic!("{e}"))
}

fn write(ir: &shadewire_nodes::MaterialIr, name: &str) -> shadewire::shader::Compiled {
    let options = MdlOptions { name: name.to_string(), ..MdlOptions::default() };
    compile(&ir.dag, &ir.source, &options).unwrap_or_else(|e| panic!("{e}"))
}

const TEXTURED: &str = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <nodegraph name="g">
    <input name="color" type="color3" value="0.8, 0.5, 0.2" />
    <input name="roughness" type="float" value="0.4" uimin="0" uimax="1" uisoftmax="0.8" uifolder="Base/Diffuse" />
    <image name="img" type="color3">
      <input name="file" type="filename" value="C:\tex\wood grain.png" colorspace="srgb_texture" />
    </image>
    <multiply name="tinted" type="color3">
      <input name="in1" type="color3" nodename="img" />
      <input name="in2" type="color3" interfacename="color" />
    </multiply>
    <output name="out_color" type="color3" nodename="tinted" />
    <dot name="r" type="float"><input name="in" type="float" interfacename="roughness" /></dot>
    <output name="out_roughness" type="float" nodename="r" />
  </nodegraph>
  <oren_nayar_diffuse_bsdf name="d" type="BSDF">
    <input name="color" type="color3" nodegraph="g" output="out_color" />
    <input name="roughness" type="float" nodegraph="g" output="out_roughness" />
  </oren_nayar_diffuse_bsdf>
  <surface name="s" type="surfaceshader"><input name="bsdf" type="BSDF" nodename="d" /></surface>
  <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s" /></surfacematerial>
</materialx>
"#;

#[test]
fn a_material_with_parameters_and_a_texture() {
    let ir = mtlx(TEXTURED, &LOBE_MODELS);
    let code = write(&ir, "m").code;
    for expected in [
        "mdl 1.7;",
        "export material m(",
        "color_ = color(0.8, 0.5, 0.2)",
        "anno::hard_range(0.0, 1.0)",
        "anno::soft_range(",
        "anno::in_group(\"Base\", \"Diffuse\")",
        "uniform texture_2d shadewire_t0 = texture_2d(\"/tex/wood grain.png\", tex::gamma_linear)",
        "tex::lookup_float4(shadewire_t0, ",
        "uniform texture_2d shadewire_t0)",
    ] {
        assert!(code.contains(expected), "{expected}\n{code}");
    }
    assert!(!code.contains("shadewire_u"), "{code}");
    let out = write(&ir, "m");
    assert!(out.reflection.approximations.iter().any(|a| a.contains("drive")), "{:?}", out.reflection.approximations);
    assert_mdlc(&code, "textured");
}

const HOST_READS: &str = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <nodegraph name="g">
    <texcoord name="uv" type="vector2" />
    <position name="pw" type="vector3"><input name="space" type="string" value="world" /></position>
    <position name="po" type="vector3"><input name="space" type="string" value="object" /></position>
    <normal name="nw" type="vector3"><input name="space" type="string" value="world" /></normal>
    <time name="t" type="float" />
    <convert name="uv3" type="vector3"><input name="in" type="vector2" nodename="uv" /></convert>
    <add name="a" type="vector3"><input name="in1" type="vector3" nodename="pw" /><input name="in2" type="vector3" nodename="po" /></add>
    <add name="b" type="vector3"><input name="in1" type="vector3" nodename="a" /><input name="in2" type="vector3" nodename="nw" /></add>
    <add name="c" type="vector3"><input name="in1" type="vector3" nodename="b" /><input name="in2" type="vector3" nodename="uv3" /></add>
    <multiply name="d" type="vector3"><input name="in1" type="vector3" nodename="c" /><input name="in2" type="float" nodename="t" /></multiply>
    <convert name="col" type="color3"><input name="in" type="vector3" nodename="d" /></convert>
    <output name="out" type="color3" nodename="col" />
  </nodegraph>
  <surface_unlit name="s" type="surfaceshader"><input name="emission_color" type="color3" nodegraph="g" output="out" /></surface_unlit>
  <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s" /></surfacematerial>
</materialx>
"#;

#[test]
fn host_reads_come_from_state() {
    let out = write(&mtlx(HOST_READS, &LOBE_MODELS), "m");
    for expected in [
        "state::texture_coordinate(0)",
        "state::position()",
        "state::normal()",
        "state::transform_point(state::coordinate_internal, state::coordinate_object, ",
        "state::animation_time()",
    ] {
        assert!(out.code.contains(expected), "{expected}\n{}", out.code);
    }
    assert!(out.reflection.approximations.is_empty(), "{:?}", out.reflection.approximations);
    assert_mdlc(&out.code, "host_reads");
}

const VIEW_AND_GEOMPROP: &str = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <nodegraph name="g">
    <viewdirection name="v" type="vector3" />
    <geompropvalue name="dc" type="vector3"><input name="geomprop" type="string" value="displayColor" /></geompropvalue>
    <add name="a" type="vector3"><input name="in1" type="vector3" nodename="v" /><input name="in2" type="vector3" nodename="dc" /></add>
    <convert name="col" type="color3"><input name="in" type="vector3" nodename="a" /></convert>
    <output name="out" type="color3" nodename="col" />
  </nodegraph>
  <surface_unlit name="s" type="surfaceshader"><input name="emission_color" type="color3" nodegraph="g" output="out" /></surface_unlit>
  <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s" /></surfacematerial>
</materialx>
"#;

#[test]
fn the_view_direction_and_geomprops_are_recorded() {
    let out = write(&mtlx(VIEW_AND_GEOMPROP, &LOBE_MODELS), "m");
    let notes = out.reflection.approximations.join("\n");
    assert!(notes.contains("view direction"), "{notes}");
    assert!(notes.contains("displayColor"), "{notes}");
    assert_mdlc(&out.code, "view_and_geomprop");
}

#[test]
fn a_named_model_is_refused() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/materialx/resources/Materials/Examples/OpenPbr");
    let file = std::fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "mtlx"))
        .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    let ir = shadewire_mtlx::to_ir(&text, None, None, &IrOptions::default(), &Default::default()).unwrap();
    match compile(&ir.dag, &ir.source, &MdlOptions::default()) {
        Err(Error::NoTranslation { from, to }) => {
            assert_eq!(from, "open_pbr_surface");
            assert_eq!(to, ["surface", "surface_unlit"]);
        }
        other => panic!("{other:?}"),
    }
}

const UNLIT: &str = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <surface_unlit name="s" type="surfaceshader">
    <input name="emission" type="float" value="2" />
    <input name="emission_color" type="color3" value="0.5, 0.25, 1" />
  </surface_unlit>
  <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s" /></surfacematerial>
</materialx>
"#;

#[test]
fn surface_unlit_is_emission() {
    let out = write(&mtlx(UNLIT, &LOBE_MODELS), "m");
    assert!(out.code.contains("material_emission(emission: df::diffuse_edf(), intensity:"), "{}", out.code);
    assert_mdlc(&out.code, "unlit");
}
