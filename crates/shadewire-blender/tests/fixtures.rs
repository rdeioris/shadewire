//! Every fixture under blender/fixtures/graphs lowers, its WGSL validates, every back end compiles it; the warnings
//! are snapshots, so a handler that lands shows up as fewer of them, and so is the MaterialX each material writes.

use std::path::{Path, PathBuf};

use shadewire::Error;
use shadewire::shader::CompileOptions;
use shadewire_blender::{IrOptions, MaterialIr, material_names, to_ir};

fn fixtures() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../blender/fixtures/graphs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    files
        .into_iter()
        .filter(|f| f.extension().is_some_and(|e| e == "json"))
        .map(|f| (f.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&f).unwrap()))
        .collect()
}

/// The two model lists a host may bring: the mapped models, and closure trees.
fn modes() -> [(&'static str, IrOptions); 2] {
    [("models", IrOptions::default()), ("lobes", IrOptions::with_models(&["surface", "surface_unlit"]))]
}

/// Lowers one fixture material; `Err(NoTranslation)` is the right answer for a closure tree under the mapped models.
fn lower(name: &str, text: &str, material: &str, mode: &str, options: &IrOptions) -> Option<MaterialIr> {
    match to_ir(text, Some(material), options) {
        Ok(ir) => Some(ir),
        Err(Error::NoTranslation { .. }) if mode == "models" && (name == "lobes" || name == "math") => None,
        Err(e) => panic!("{name}/{material} ({mode}): {e}"),
    }
}

#[test]
fn every_fixture_lowers_and_its_warnings_are_the_snapshots() {
    for (name, text) in fixtures() {
        for material in material_names(&text).unwrap() {
            for (mode, options) in modes() {
                let Some(ir) = lower(&name, &text, &material, mode, &options) else { continue };
                insta::assert_snapshot!(format!("{name}_{material}_{mode}_warnings"), ir.warnings.join("\n"));
                // The mapped models' documents, and the closure trees' of the fixtures that only make closure trees.
                let snapshot = match mode {
                    "models" => Some(format!("{name}_{material}_mtlx")),
                    _ if name == "lobes" || name == "math" => Some(format!("{name}_{material}_{mode}_mtlx")),
                    _ => None,
                };
                if let Some(snapshot) = snapshot {
                    let write = shadewire_mtlx::WriteOptions { name: material.clone(), ..Default::default() };
                    let written = shadewire_mtlx::from_ir(&ir.dag, &write).unwrap();
                    insta::assert_snapshot!(snapshot, written.text);
                }
                assert_eq!(ir.source.source, "blender");
            }
        }
    }
}

#[test]
fn every_fixture_compiles_in_every_back_end_and_validates_as_wgsl() {
    let options = CompileOptions::default();
    for (name, text) in fixtures() {
        for material in material_names(&text).unwrap() {
            for (mode, lower_options) in modes() {
                let Some(ir) = lower(&name, &text, &material, mode, &lower_options) else { continue };
                let wgsl =
                    shadewire_wgsl::compile(&ir.dag, &ir.source, &options).unwrap_or_else(|e| panic!("{name}: {e}"));
                shadewire_wgsl::validate::check(&wgsl.code, &wgsl.reflection)
                    .unwrap_or_else(|e| panic!("{name}/{material} ({mode}): the WGSL does not validate:\n{e}"));
                let version = shadewire_glsl::Version::Desktop(420);
                shadewire_glsl::compile(&ir.dag, &ir.source, &options, version)
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
                shadewire_hlsl::compile(&ir.dag, &ir.source, &options).unwrap_or_else(|e| panic!("{name}: {e}"));
                shadewire_msl::compile(&ir.dag, &ir.source, &options).unwrap_or_else(|e| panic!("{name}: {e}"));
            }
        }
    }
}
