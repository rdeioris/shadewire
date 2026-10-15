//! The tests' composition of the MaterialX front end and the WGSL back end, and the writer's round trip.
// Each test binary uses part of this module.
#![allow(dead_code)]

use std::path::Path;

use shadewire::shader::{CompileOptions, Compiled};
#[allow(unused_imports)]
pub use shadewire::testing::{
    Probe, ProbeTextures, assert_same_evaluation, assert_same_material, assert_same_material_but_texture_colorspaces,
    probes,
};
use shadewire::{Dag, Error};
use shadewire_mtlx::{Resolver, WriteOptions, from_ir, to_ir};
use shadewire_nodes::IrOptions;
use shadewire_wgsl::compile;

/// `to_ir`, then `compile`.
pub fn compile_mtlx(
    text: &str,
    file: Option<&Path>,
    material: Option<&str>,
    lower: &IrOptions,
    options: &CompileOptions,
) -> Result<Compiled, Error> {
    let ir = to_ir(text, file.and_then(Path::to_str), material, lower, &Resolver::default())?;
    compile(&ir.dag, &ir.source, options)
}

/// `roundtrip_material` of the document's first material.
pub fn roundtrip(text: &str, options: &IrOptions, write: &WriteOptions) -> (Dag, String, Dag) {
    roundtrip_material(text, None, None, options, write)
}

/// `a = to_ir(text)`, its written document, and `b = to_ir(written)`; panics with the written text when `b` does
/// not read.
pub fn roundtrip_material(
    text: &str,
    location: Option<&str>,
    material: Option<&str>,
    options: &IrOptions,
    write: &WriteOptions,
) -> (Dag, String, Dag) {
    let a = to_ir(text, location, material, options, &Resolver::default())
        .unwrap_or_else(|e| panic!("{location:?} {material:?}: {e}"))
        .dag;
    let written = from_ir(&a, write).unwrap_or_else(|e| panic!("{location:?} {material:?}: writing: {e}"));
    let b = to_ir(&written.text, None, None, options, &Resolver::default())
        .unwrap_or_else(|e| panic!("{location:?} {material:?}: reading the written document: {e}\n{}", written.text))
        .dag;
    (a, written.text, b)
}
