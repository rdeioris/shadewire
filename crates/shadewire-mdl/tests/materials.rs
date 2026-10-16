//! The df-to-lobe mapping, one fixture material per row of the spec's table: the closure tree and the warnings of
//! each are snapshots.

use std::path::{Path, PathBuf};

use shadewire::optimize::optimize;
use shadewire::{Error, LOBE_MODELS, SourceInfo};
use shadewire_mdl::{IrOptions, Resolver, material_names, to_ir};

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
