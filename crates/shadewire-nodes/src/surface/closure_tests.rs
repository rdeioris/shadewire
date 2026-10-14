use shadewire::optimize::optimize;
use shadewire::{ClosureNode, ClosureType, Dag, Error};

use super::models::KNOWN_MODELS;
use crate::document::Document;
use crate::document::build::build;
use crate::library::standard;
use crate::parse::parse_str;
use crate::{ElaborateOptions, elaborate};

fn inline(version: &str, body: &str) -> Document {
    let text = format!(r#"<materialx version="{version}">{body}</materialx>"#);
    build(&parse_str(&text, None).unwrap(), Some("test.mtlx")).unwrap()
}

fn compile(doc: &Document, material: Option<&str>, models: &[&str]) -> Result<Dag, Error> {
    elaborate(doc, standard(), &ElaborateOptions::default(), material, models)
}

/// The teapot's porcelain from pablode/cg-assets (CC0): a glaze layered over a diffuse and subsurface mix.
/// Authored as 1.39 (`radius` a color3): the 1.38 original needs `shadewire-mtlx`'s upgrade, which this crate does not
/// have.
const PORCELAIN: &str = r#"
  <nodegraph name="NG_porcelain">
    <subsurface_bsdf name="node1" type="BSDF">
      <input name="color" type="color3" value="0.75, 0.75, 0.75" />
    </subsurface_bsdf>
    <subsurface_bsdf name="node2" type="BSDF">
      <input name="radius" type="color3" value="0.3, 0.3, 0.3" />
      <input name="color" type="color3" value="0.75, 0.75, 0.75" />
    </subsurface_bsdf>
    <mix name="node3" type="BSDF">
      <input name="bg" type="BSDF" nodename="node1" />
      <input name="fg" type="BSDF" nodename="node2" />
      <input name="mix" type="float" value="0.35" />
    </mix>
    <oren_nayar_diffuse_bsdf name="node4" type="BSDF">
      <input name="color" type="color3" value="0.9, 0.9, 0.9" />
      <input name="roughness" type="float" value="0.65" />
    </oren_nayar_diffuse_bsdf>
    <mix name="node5" type="BSDF">
      <input name="bg" type="BSDF" nodename="node4" />
      <input name="fg" type="BSDF" nodename="node3" />
      <input name="mix" type="float" value="0.35" />
    </mix>
    <dielectric_bsdf name="node6" type="BSDF">
      <input name="weight" type="float" value="0.35" />
    </dielectric_bsdf>
    <layer name="node7" type="BSDF">
      <input name="top" type="BSDF" nodename="node6" />
      <input name="base" type="BSDF" nodename="node5" />
    </layer>
    <output name="out_bsdf" type="BSDF" nodename="node7" />
  </nodegraph>
  <surface name="SR_porcelain" type="surfaceshader">
    <input name="bsdf" type="BSDF" nodegraph="NG_porcelain" output="out_bsdf" />
  </surface>
  <surfacematerial name="porcelain" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR_porcelain" />
  </surfacematerial>
"#;

fn lobe_kinds(dag: &Dag) -> Vec<String> {
    let closure = dag.surface.closure.as_ref().unwrap();
    closure
        .nodes
        .iter()
        .filter_map(|n| match n {
            ClosureNode::Lobe { kind, .. } => Some(kind.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_surface_node_is_unsupported_unless_the_host_accepts_closures() {
    let doc = inline("1.39", PORCELAIN);
    let err = compile(&doc, None, &KNOWN_MODELS).unwrap_err();
    assert!(err.to_string().contains("not a known shading model"), "{err}");
}

#[test]
fn the_porcelain_compiles_to_its_closure_tree() {
    let doc = inline("1.39", PORCELAIN);
    let dag = compile(&doc, None, &["surface"]).unwrap();
    assert_eq!(dag.surface.model, "surface");
    let closure = dag.surface.closure.as_ref().unwrap();
    let dump = closure.dump();
    assert!(dump.contains("c6 = layer(top=c0, base=c5)\nbsdf = c6\nedf = none\nopacity = opacity\n"), "{dump}");
    assert!(dump.contains("c3 = mix(fg=c1, bg=c2, mix=c3_mix)"), "{dump}");
    assert!(dump.contains("c5 = mix(fg=c3, bg=c4, mix=c5_mix)"), "{dump}");
    assert!(dump.contains("c0 = dielectric_bsdf("), "{dump}");
    assert!(dump.contains("scatter_mode=\"R\""), "{dump}");
    assert_eq!(lobe_kinds(&dag), ["dielectric_bsdf", "subsurface_bsdf", "subsurface_bsdf", "oren_nayar_diffuse_bsdf"]);
    assert!(dump.contains("c1 = subsurface_bsdf(") && dump.contains("radius=c1_radius"), "{dump}");
    let names: Vec<&str> = dag.surface.fields.iter().map(|f| f.name.as_str()).collect();
    for field in ["c0_weight", "c0_ior", "c0_normal", "c3_mix", "c4_roughness", "opacity", "thin_walled"] {
        assert!(names.contains(&field), "{field} missing from {names:?}");
    }
    for node in &closure.nodes {
        for field in node.fields() {
            assert!(names.contains(&field), "{field} is not a surface field");
        }
    }
}

#[test]
fn a_named_model_opens_when_only_closures_are_accepted() {
    let doc = inline(
        "1.39",
        r#"<gltf_pbr name="pbr" type="surfaceshader">
             <input name="base_color" type="color3" value="0.5, 0.2, 0.1" />
           </gltf_pbr>
           <surfacematerial name="m" type="material">
             <input name="surfaceshader" type="surfaceshader" nodename="pbr" />
           </surfacematerial>"#,
    );
    let opened = compile(&doc, None, &["surface"]).unwrap();
    assert_eq!(opened.surface.model, "surface");
    let kinds = lobe_kinds(&opened);
    for kind in ["oren_nayar_diffuse_bsdf", "dielectric_bsdf", "generalized_schlick_bsdf", "sheen_bsdf", "uniform_edf"]
    {
        assert!(kinds.contains(&kind.to_string()), "{kind} missing from {kinds:?}");
    }
    assert!(opened.surface.closure.as_ref().unwrap().edf.is_some());
    let kept = compile(&doc, None, &["gltf_pbr", "surface"]).unwrap();
    assert_eq!(kept.surface.model, "gltf_pbr");
    assert!(kept.surface.closure.is_none());
}

#[test]
fn usd_preview_surface_opens_into_closures() {
    let doc = inline(
        "1.38",
        r#"<UsdPreviewSurface name="SR_light" type="surfaceshader">
             <input name="emissiveColor" type="color3" value="12.0, 12.0, 12.0" />
           </UsdPreviewSurface>
           <surfacematerial name="light" type="material">
             <input name="surfaceshader" type="surfaceshader" nodename="SR_light" />
           </surfacematerial>"#,
    );
    let dag = compile(&doc, None, &["surface"]).unwrap();
    let closure = dag.surface.closure.as_ref().unwrap();
    let Some(edf) = closure.edf else { panic!("no edf: {}", closure.dump()) };
    let ClosureNode::Lobe { kind, ty, .. } = &closure.nodes[edf as usize] else { panic!("{}", closure.dump()) };
    assert_eq!((kind.as_str(), *ty), ("uniform_edf", ClosureType::Edf));
}

#[test]
fn an_edf_alone_makes_an_emitter_without_a_bsdf() {
    let doc = inline(
        "1.38",
        r#"<uniform_edf name="e" type="EDF"><input name="color" type="color3" value="1000, 1000, 1000" /></uniform_edf>
           <surface name="s" type="surfaceshader"><input name="edf" type="EDF" nodename="e" /></surface>
           <surfacematerial name="Light" type="material">
             <input name="surfaceshader" type="surfaceshader" nodename="s" />
           </surfacematerial>"#,
    );
    let dag = optimize(&compile(&doc, None, &["surface"]).unwrap());
    let closure = dag.surface.closure.as_ref().unwrap();
    assert_eq!(closure.bsdf, None);
    assert_eq!(closure.dump(), "c0 = uniform_edf(color=c0_color)\nbsdf = none\nedf = c0\nopacity = opacity\n");
}

#[test]
fn a_mix_with_one_side_unconnected_scales_the_other() {
    let doc = inline(
        "1.39",
        r#"<oren_nayar_diffuse_bsdf name="d" type="BSDF" />
           <mix name="m" type="BSDF">
             <input name="fg" type="BSDF" nodename="d" />
             <input name="mix" type="float" value="0.25" />
           </mix>
           <surface name="s" type="surfaceshader"><input name="bsdf" type="BSDF" nodename="m" /></surface>
           <surfacematerial name="M" type="material">
             <input name="surfaceshader" type="surfaceshader" nodename="s" />
           </surfacematerial>"#,
    );
    let dag = compile(&doc, None, &["surface"]).unwrap();
    let dump = dag.surface.closure.as_ref().unwrap().dump();
    assert!(dump.contains("c1 = multiply(c0, c1_in2)\nbsdf = c1"), "{dump}");
}

#[test]
fn a_mix_of_two_surfaces_mixes_their_closures() {
    let doc = inline(
        "1.39",
        r#"<oren_nayar_diffuse_bsdf name="d" type="BSDF" />
           <conductor_bsdf name="c" type="BSDF" />
           <surface name="s1" type="surfaceshader"><input name="bsdf" type="BSDF" nodename="d" /></surface>
           <surface name="s2" type="surfaceshader"><input name="bsdf" type="BSDF" nodename="c" /></surface>
           <mix name="sm" type="surfaceshader">
             <input name="fg" type="surfaceshader" nodename="s1" />
             <input name="bg" type="surfaceshader" nodename="s2" />
             <input name="mix" type="float" value="0.5" />
           </mix>
           <surfacematerial name="M" type="material">
             <input name="surfaceshader" type="surfaceshader" nodename="sm" />
           </surfacematerial>"#,
    );
    let dag = compile(&doc, None, &["surface"]).unwrap();
    let dump = dag.surface.closure.as_ref().unwrap().dump();
    assert!(dump.contains("c2 = mix(fg=c0, bg=c1, mix=c2_mix)\nbsdf = c2"), "{dump}");
    assert_eq!(dag.surface.fields.iter().filter(|f| f.name == "opacity").count(), 1);
}
