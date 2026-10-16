//! MaterialX → IR → MDL → IR: what the writer writes, the reader reads back to the same lobes with the same weights
//! and inputs at every probe, except where the spec's tables approximate.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::{FlatLobe, flat_lobes};
use shadewire::optimize::optimize;
use shadewire::testing::probes;
use shadewire::{Dag, LOBE_MODELS, NodeKind, Type};
use shadewire_mdl::{IrOptions, MdlOptions, Resolver, compile, write::mdl_ident};

/// Lobe inputs the round trip changes on purpose, each with the spec row it comes from.
const APPROXIMATED: &[(&str, &str)] = &[
    // Writer, generalized_schlick_bsdf: MDL 1.7's color_custom_curve_layer has no F82; the grazing color takes it.
    ("generalized_schlick_bsdf", "color82"),
    ("generalized_schlick_bsdf", "color90"),
    // Writer, subsurface_bsdf: written as diffuse reflection.
    ("subsurface_bsdf", "*"),
    // Reader, the diffuse row: MDL has one diffuse lobe, read as Oren-Nayar.
    ("burley_diffuse_bsdf", "*"),
    // Writer: MDL 1.7's diffuse has no energy compensation.
    ("oren_nayar_diffuse_bsdf", "energy_compensation"),
];

/// Lobes the writer leaves out (the spec's writer table: a Schlick EDF is written as its base, hair not at all;
/// volumes are out of scope).
const DROPPED: &[&str] = &["generalized_schlick_edf", "chiang_hair_bsdf", "anisotropic_vdf", "absorption_vdf"];

fn approximated(kind: &str, input: &str) -> bool {
    APPROXIMATED.iter().any(|(k, i)| *k == kind && (*i == "*" || *i == input))
}

/// The kind the reader reads a written lobe back as.
fn read_back_kind(kind: &str) -> &str {
    match kind {
        "burley_diffuse_bsdf" | "subsurface_bsdf" => "oren_nayar_diffuse_bsdf",
        other => other,
    }
}

fn close(a: &[f32], b: &[f32]) -> bool {
    let same = |(x, y): (&f32, &f32)| (x - y).abs() <= 1e-4 * (1.0 + x.abs()) || (x.is_nan() && y.is_nan());
    a.len() == b.len() && a.iter().zip(b).all(same)
}

/// The uniforms a written module keeps as parameters, under the names the writer gives them.
fn uniform_names(dag: &Dag, written: bool) -> BTreeSet<String> {
    dag.nodes
        .iter()
        .filter(|n| n.ty != Type::Integer)
        .filter_map(|n| match &n.kind {
            NodeKind::Uniform { name, .. } => Some(if written { name.clone() } else { mdl_ident(name) }),
            _ => None,
        })
        .collect()
}

fn compare(a: &[FlatLobe], b: &[FlatLobe], what: &str) {
    let a: Vec<FlatLobe> = a.iter().filter(|l| !DROPPED.contains(&l.kind.as_str())).cloned().collect();
    let kinds = |l: &[FlatLobe]| l.iter().map(|x| read_back_kind(&x.kind).to_string()).collect::<Vec<_>>();
    assert_eq!(kinds(&a), kinds(b), "{what}: lobes");
    for (x, y) in a.iter().zip(b) {
        if approximated(&x.kind, "*") {
            continue;
        }
        assert!(close(&x.weight, &y.weight), "{what}: {} weight {:?} became {:?}", x.kind, x.weight, y.weight);
        for (input, value) in &x.inputs {
            if approximated(&x.kind, input) {
                continue;
            }
            let Some(other) = y.inputs.get(input) else { continue };
            assert!(close(value, other), "{what}: {}.{input} {value:?} became {other:?}", x.kind);
        }
    }
}

#[test]
fn every_example_survives_mdl() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/materialx/resources/Materials/Examples");
    let options = IrOptions::with_models(&LOBE_MODELS);
    let mut count = 0;
    for folder in ["StandardSurface", "OpenPbr", "GltfPbr"] {
        let mut files: Vec<PathBuf> =
            std::fs::read_dir(root.join(folder)).unwrap().map(|e| e.unwrap().path()).collect();
        files.sort();
        for file in files.into_iter().filter(|f| f.extension().is_some_and(|e| e == "mtlx")) {
            let text = std::fs::read_to_string(&file).unwrap();
            let location = file.display().to_string();
            for material in shadewire_mtlx::material_names(&text, Some(&location), &Default::default()).unwrap() {
                let a = shadewire_mtlx::to_ir(&text, Some(&location), Some(&material), &options, &Default::default())
                    .unwrap();
                let name = mdl_ident(&material);
                let written =
                    compile(&a.dag, &a.source, &MdlOptions { name: name.clone(), ..Default::default() }).unwrap();
                let b = shadewire_mdl::to_ir(&written.code, None, Some(&name), &options, &Resolver::default())
                    .unwrap_or_else(|e| panic!("{location} {material}: reading back: {e}\n{}", written.code));
                let what = format!("{location} {material}");
                // The writer writes the optimized dag: its zero-weight lobes and unread uniforms are gone.
                let (da, db) = (optimize(&a.dag), optimize(&b.dag));
                assert_eq!(uniform_names(&da, false), uniform_names(&b.dag, true), "{what}: uniforms");
                for mut probe in probes() {
                    let (x, y) = (flat_lobes(&da, &mut probe), flat_lobes(&db, &mut probe));
                    compare(&x, &y, &format!("{what} (probe {})", probe.0));
                }
                count += 1;
            }
        }
    }
    assert!(count >= 54, "{count} materials");
}
