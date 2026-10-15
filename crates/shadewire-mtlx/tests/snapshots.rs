//! Snapshots of the DAG dump and the reflection JSON of a few reference materials:
//! the default material of each surface model, a procedural and a textured example; and of the documents the
//! writer makes of a few of them.

use std::path::{Path, PathBuf};

mod common;

use common::compile_mtlx;
use shadewire_mtlx::WriteOptions;
use shadewire_nodes::IrOptions;
use shadewire_wgsl::{CompileOptions, Compiled};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Compiles a file given relative to the repository root, with the default options.
fn compile(relative: &str) -> Compiled {
    let path = root().join(relative);
    let text = std::fs::read_to_string(&path).unwrap();
    compile_mtlx(&text, Some(&path), None, &IrOptions::default(), &CompileOptions::default())
        .unwrap_or_else(|e| panic!("{relative}: {e}"))
}

fn snapshot(name: &str, relative: &str) {
    let compiled = compile(relative);
    insta::assert_snapshot!(format!("{name}_dag"), compiled.dag.dump());
    insta::assert_snapshot!(format!("{name}_reflection"), serde_json::to_string_pretty(&compiled.reflection).unwrap());
}

const EXAMPLES: &str = "vendor/materialx/resources/Materials/Examples";

#[test]
fn standard_surface_default() {
    snapshot("standard_surface_default", &format!("{EXAMPLES}/StandardSurface/standard_surface_default.mtlx"));
}

#[test]
fn open_pbr_default() {
    snapshot("open_pbr_default", &format!("{EXAMPLES}/OpenPbr/open_pbr_default.mtlx"));
}

#[test]
fn gltf_pbr_default() {
    snapshot("gltf_pbr_default", &format!("{EXAMPLES}/GltfPbr/gltf_pbr_default.mtlx"));
}

#[test]
fn surface_unlit_basic() {
    snapshot("unlit_basic", "tools/oracle/materials/unlit_basic.mtlx");
}

#[test]
fn standard_surface_marble_solid() {
    snapshot(
        "standard_surface_marble_solid",
        &format!("{EXAMPLES}/StandardSurface/standard_surface_marble_solid.mtlx"),
    );
}

#[test]
fn standard_surface_brass_tiled() {
    snapshot("standard_surface_brass_tiled", &format!("{EXAMPLES}/StandardSurface/standard_surface_brass_tiled.mtlx"));
}

/// The document `from_ir` writes for material `material` of a file given relative to the repository root.
fn written(relative: &str, material: &str, options: &IrOptions) -> String {
    let path = root().join(relative);
    let text = std::fs::read_to_string(&path).unwrap();
    let ir = shadewire_mtlx::to_ir(&text, path.to_str(), Some(material), options, &Default::default()).unwrap();
    let write = WriteOptions { name: material.to_string(), ..WriteOptions::default() };
    shadewire_mtlx::from_ir(&ir.dag, &write).unwrap_or_else(|e| panic!("{relative}: {e}")).text
}

#[test]
fn written_open_pbr_default() {
    let path = format!("{EXAMPLES}/OpenPbr/open_pbr_default.mtlx");
    insta::assert_snapshot!(written(&path, "Default", &IrOptions::default()));
}

#[test]
fn written_standard_surface_marble_solid() {
    let path = format!("{EXAMPLES}/StandardSurface/standard_surface_marble_solid.mtlx");
    insta::assert_snapshot!(written(&path, "Marble_3D", &IrOptions::default()));
}

#[test]
fn written_gltf_pbr_default() {
    let path = format!("{EXAMPLES}/GltfPbr/gltf_pbr_default.mtlx");
    insta::assert_snapshot!(written(&path, "Default", &IrOptions::default()));
}

#[test]
fn written_porcelain() {
    let lobes = IrOptions::with_models(&["surface", "surface_unlit"]);
    insta::assert_snapshot!(written("crates/shadewire-mtlx/tests/fixtures/porcelain.mtlx", "porcelain", &lobes));
}

#[test]
fn compositing() {
    snapshot("compositing", "tools/oracle/materials/compositing.mtlx");
}

#[test]
fn fractal2d() {
    snapshot("fractal2d", "tools/oracle/materials/fractal2d.mtlx");
}

#[test]
fn normalmap_object_1_38() {
    snapshot("normalmap_object", "tools/oracle/materials/normalmap_object.mtlx");
}
