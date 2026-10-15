//! The writer's acceptance: every material `to_ir` reads survives `from_ir` and a second `to_ir`.

mod common;
use common::{assert_same_material, roundtrip};
use shadewire_mtlx::WriteOptions;
use shadewire_nodes::IrOptions;
use std::path::{Path, PathBuf};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/materialx/resources/Materials/Examples")
}

/// Every example that `to_ir` accepts, under both model lists and both writer modes.
#[test]
fn every_readable_example_survives_a_round_trip() {
    let mut count = 0;
    for folder in ["StandardSurface", "OpenPbr", "GltfPbr"] {
        let mut files: Vec<_> =
            std::fs::read_dir(examples().join(folder)).unwrap().map(|e| e.unwrap().path()).collect();
        files.sort();
        for file in files.into_iter().filter(|f| f.extension().is_some_and(|e| e == "mtlx")) {
            if file.ends_with("standard_surface_onyx_hextiled.mtlx") {
                continue; // hextiling's Dpdx/Dpdy have no nodedef to write until Shadewire's own are declared
            }
            let text = std::fs::read_to_string(&file).unwrap();
            let location = file.to_str().unwrap();
            for options in [IrOptions::default(), IrOptions::with_models(&["surface", "surface_unlit"])] {
                let Ok(names) = shadewire_mtlx::material_names(&text, Some(location), &Default::default()) else {
                    continue;
                };
                for material in names {
                    if shadewire_mtlx::to_ir(&text, Some(location), Some(&material), &options, &Default::default())
                        .is_err()
                    {
                        continue; // allow-listed in examples.rs; the writer needs a dag to write
                    }
                    for inline in [false, true] {
                        let write = WriteOptions { inline, name: material.clone(), ..WriteOptions::default() };
                        let (a, written, b) =
                            common::roundtrip_material(&text, Some(location), Some(&material), &options, &write);
                        assert_same_material(&a, &b, &written);
                        count += 1;
                    }
                }
            }
        }
    }
    assert!(count > 40, "{count} round trips");
}

const PORCELAIN: &str = include_str!("fixtures/porcelain.mtlx"); // copy of the 1.39 PORCELAIN document from shadewire-nodes' closure_tests.rs
const UNIFORMS_AND_IMAGE: &str = include_str!("fixtures/uniforms_and_image.mtlx"); // the image/uniform document of Task 3's tests with a uniform nodegraph input

#[test]
fn closure_trees_and_uniforms_survive_a_round_trip() {
    for (doc, models) in [(PORCELAIN, &["surface", "surface_unlit"][..]), (UNIFORMS_AND_IMAGE, &["surface_unlit"][..])]
    {
        let options = IrOptions::with_models(models);
        for inline in [false, true] {
            let (a, written, b) = roundtrip(doc, &options, &WriteOptions { inline, ..WriteOptions::default() });
            assert_same_material(&a, &b, &written);
        }
    }
}

#[test]
fn a_user_nodedef_compound_is_expanded_with_a_warning() {
    let text = r#"<materialx version="1.39">
      <nodedef name="ND_twice" node="twice"><input name="in" type="float" value="0"/><output name="out" type="float"/></nodedef>
      <nodegraph name="NG_twice" nodedef="ND_twice">
        <multiply name="m" type="float"><input name="in1" type="float" interfacename="in"/><input name="in2" type="float" value="2"/></multiply>
        <output name="out" type="float" nodename="m"/>
      </nodegraph>
      <nodegraph name="g"><twice name="t" type="float"><input name="in" type="float" value="0.25"/></twice><output name="out" type="float" nodename="t"/></nodegraph>
      <surface_unlit name="s" type="surfaceshader"><input name="emission" type="float" nodegraph="g" output="out"/></surface_unlit>
      <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>
    </materialx>"#;
    let ir = shadewire_mtlx::to_ir(text, None, None, &IrOptions::default(), &Default::default()).unwrap();
    let written = shadewire_mtlx::from_ir(&ir.dag, &WriteOptions::default()).unwrap();
    assert!(written.text.contains("<multiply"), "{}", written.text);
    assert!(!written.text.contains("<twice"), "{}", written.text);
    assert_eq!(written.warnings, ["compound ND_twice at g/t written expanded: not a library nodedef"]);
}
