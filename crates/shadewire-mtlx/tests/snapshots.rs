//! Snapshots of the DAG dump and the reflection JSON of a few reference materials:
//! the default material of each surface model, a procedural and a textured example.

use std::path::{Path, PathBuf};

mod common;

use common::compile_mtlx;
use shadewire_mtlx::MtlxOptions;
use shadewire_wgsl::{CompileOptions, Compiled};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Compiles a file given relative to the repository root, with the default options.
fn compile(relative: &str) -> Compiled {
    let path = root().join(relative);
    let text = std::fs::read_to_string(&path).unwrap();
    compile_mtlx(&text, Some(&path), None, &MtlxOptions::default(), &CompileOptions::default())
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
