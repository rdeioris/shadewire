//! The df-to-lobe mapping, one fixture material per row of the spec's table: the closure tree and the warnings of
//! each are snapshots.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::{Scatter, bsdf_scatter};
use shadewire::optimize::optimize;
use shadewire::testing::probes;
use shadewire::{ClosureNode, Dag, Error, LOBE_MODELS, SourceInfo, Value};
use shadewire_mdl::{ArgValue, Argument, IrOptions, Modules, Resolver, material_names, to_ir};

fn fixtures() -> Vec<(String, PathBuf)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    files
        .into_iter()
        .filter(|f| f.extension().is_some_and(|e| e == "mdl"))
        .map(|f| (f.file_stem().unwrap().to_string_lossy().to_string(), f))
        .collect()
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}.mdl"))).unwrap()
}

/// Each surface field and what holds it: a constant's value, a uniform's name, or `computed`.
fn field_values(dag: &shadewire::Dag) -> String {
    let mut out = String::new();
    for f in &dag.surface.fields {
        let held = match &dag.node(f.node).kind {
            shadewire::NodeKind::Const(v) => format!("{:?}", v.floats()),
            shadewire::NodeKind::Uniform { name, .. } => format!("uniform {name}"),
            shadewire::NodeKind::Context(read) => format!("context {}", read.reflection_name()),
            _ => "computed".to_string(),
        };
        out.push_str(&format!("{} = {held}\n", f.name));
    }
    out
}

#[test]
fn every_fixture_material_snapshots() {
    let options = IrOptions::with_models(&LOBE_MODELS);
    let resolver = Resolver::default();
    for (file, path) in fixtures() {
        let text = std::fs::read_to_string(&path).unwrap();
        for material in material_names(&text, None, &resolver).unwrap() {
            let ir = to_ir(&text, None, Some(&material), &options, &resolver)
                .unwrap_or_else(|e| panic!("{file}::{material}: {e}"));
            let dag = optimize(&ir.dag);
            let closure = dag.surface.closure.as_ref().expect("a closure tree").dump();
            let fields = field_values(&dag);
            let warnings = ir.warnings.join("\n");
            let text = format!("{closure}--- fields\n{fields}--- warnings\n{warnings}");
            insta::assert_snapshot!(format!("{file}_{material}"), text);
        }
    }
}

#[test]
fn default_models_refuse_an_mdl_material() {
    let result = to_ir(&fixture("diffuse"), None, Some("reflection"), &IrOptions::default(), &Resolver::default());
    assert!(matches!(result, Err(Error::NoTranslation { ref from, .. }) if from == "surface"), "{result:?}");
}

#[test]
fn source_names_the_mdl_version() {
    let options = IrOptions::with_models(&LOBE_MODELS);
    let ir = to_ir(&fixture("diffuse"), None, Some("reflection"), &options, &Resolver::default()).unwrap();
    assert_eq!(ir.source, SourceInfo { source: "mdl".into(), mtlx_version: "MDL 1.7".into() });
}

/// The geometry normal reaches the lobes: their normal is not the plain shading normal.
#[test]
fn a_geometry_normal_reaches_the_lobes() {
    let options = IrOptions::with_models(&LOBE_MODELS);
    let ir = to_ir(&fixture("geometry"), None, Some("normal_mapped"), &options, &Resolver::default()).unwrap();
    let dag = optimize(&ir.dag);
    let normal = dag.surface.fields.iter().find(|f| f.name == "c0_normal").expect("a normal field");
    assert!(!matches!(dag.node(normal.node).kind, shadewire::NodeKind::Context(_)), "{}", dag.dump());
}

/// Material `material` of fixture `weighted`, `w` given.
fn weighted(material: &str, w: Option<f32>) -> shadewire_mdl::MaterialIr {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let modules = Modules::new(Resolver::new(vec![dir])).unwrap();
    let args: Vec<Argument> =
        w.map(|w| Argument { name: "w".into(), value: ArgValue::Value(Value::Float(w)) }).into_iter().collect();
    modules.to_ir("::weighted", Some(material), &args, &IrOptions::with_models(&LOBE_MODELS)).unwrap()
}

/// Opaque stand-ins for the fixture's lobes: a diffuse lobe scatters its color, a conductor 0.9.
fn opaque(kind: &str, inputs: &BTreeMap<String, Vec<f32>>) -> Scatter {
    let weight = inputs.get("weight").map_or(1.0, |w| w[0]);
    let response = match kind {
        "oren_nayar_diffuse_bsdf" => inputs["color"][0],
        "conductor_bsdf" => 0.9,
        other => panic!("no stand-in for {other}"),
    };
    ([response * weight; 3], [0.0; 3])
}

fn scatter(dag: &Dag) -> Scatter {
    let mut probe = probes().into_iter().next().unwrap();
    bsdf_scatter(dag, &mut probe, &opaque)
}

fn near(a: [f32; 3], b: f32) -> bool {
    a.iter().all(|x| (x - b).abs() <= 1e-3)
}

/// `weighted_layer(0, conductor, diffuse)` is the diffuse base: the conductor's zero weight does not block it.
#[test]
fn a_weighted_layer_of_weight_zero_is_its_base() {
    let (response, throughput) = scatter(&weighted("over_diffuse", Some(0.0)).dag);
    assert!(near(response, 0.5), "{response:?}");
    assert!(near(throughput, 0.0), "{throughput:?}");
}

/// At weight 0.5, half the conductor and the base scaled by 0.5 + 0.5 × the conductor's throughput (0).
#[test]
fn a_weighted_layer_mixes_the_layered_top_with_its_base() {
    let ir = weighted("over_diffuse", Some(0.5));
    let (response, _) = scatter(&ir.dag);
    assert!(near(response, 0.5 * 0.9 + 0.5 * 0.5), "{response:?}");
    let graph = ir.dag.surface.closure.as_ref().unwrap();
    let ClosureNode::Mix { fg, bg, .. } = &graph.nodes[graph.bsdf.unwrap() as usize] else {
        panic!("{}", graph.dump())
    };
    assert!(matches!(&graph.nodes[*fg as usize], ClosureNode::Layer { base, .. } if base == bg), "{}", graph.dump());
}

/// Known weights summing to one: nested mixes, opaque throughout, the response the weighted sum.
#[test]
fn a_mix_of_weights_summing_to_one_nests_mixes() {
    let ir = weighted("split", None);
    let graph = ir.dag.surface.closure.as_ref().unwrap();
    assert!(matches!(graph.nodes[graph.bsdf.unwrap() as usize], ClosureNode::Mix { .. }), "{}", graph.dump());
    let (response, throughput) = scatter(&ir.dag);
    assert!(near(response, 0.3 * 0.5 + 0.7 * 0.9), "{response:?}");
    assert!(near(throughput, 0.0), "{throughput:?}");
}

/// Weights summing to less than one stay a sum; one with a transmitting lobe warns.
#[test]
fn a_partial_mix_of_transmissive_lobes_warns() {
    let ir = weighted("partial", None);
    let graph = ir.dag.surface.closure.as_ref().unwrap();
    assert!(matches!(graph.nodes[graph.bsdf.unwrap() as usize], ClosureNode::Add { .. }), "{}", graph.dump());
    assert!(ir.warnings.iter().any(|w| w.ends_with("a mix of transmissive lobes approximated")), "{:?}", ir.warnings);
    assert!(weighted("split", None).warnings.is_empty());
}

/// A layer's weight is limited to [0, 1], as MDL's `hard_range` has it: 1.2 is the layered top, -0.3 the base alone.
#[test]
fn a_weighted_layer_clamps_its_weight() {
    let (response, _) = scatter(&weighted("over_diffuse", Some(1.2)).dag);
    assert!(near(response, 0.9), "{response:?}");
    let (response, _) = scatter(&weighted("over_diffuse", Some(-0.3)).dag);
    assert!(near(response, 0.5), "{response:?}");
    let ir = weighted("known_over_diffuse", None);
    let graph = ir.dag.surface.closure.as_ref().unwrap();
    assert!(matches!(graph.nodes[graph.bsdf.unwrap() as usize], ClosureNode::Layer { .. }), "{}", graph.dump());
}
