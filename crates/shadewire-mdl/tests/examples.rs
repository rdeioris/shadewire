//! Every vendored MaterialX example, opened into its closure tree, compiles to MDL, and `mdlc`
//! accepts it when installed.

mod common;

use std::path::{Path, PathBuf};

use shadewire::LOBE_MODELS;
use shadewire_mdl::{IrOptions, MdlOptions, compile, write::mdl_ident};

/// `(file, material)` of every example material, with its text.
pub fn examples() -> Vec<(PathBuf, String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/materialx/resources/Materials/Examples");
    let mut out = Vec::new();
    for folder in ["StandardSurface", "OpenPbr", "GltfPbr"] {
        let mut files: Vec<PathBuf> =
            std::fs::read_dir(root.join(folder)).unwrap().map(|e| e.unwrap().path()).collect();
        files.sort();
        for file in files.into_iter().filter(|f| f.extension().is_some_and(|e| e == "mtlx")) {
            let text = std::fs::read_to_string(&file).unwrap();
            let location = file.display().to_string();
            let Ok(names) = shadewire_mtlx::material_names(&text, Some(&location), &Default::default()) else {
                continue;
            };
            for name in names {
                out.push((file.clone(), name, text.clone()));
            }
        }
    }
    out
}

#[test]
fn every_example_compiles_and_passes_mdlc() {
    let options = IrOptions::with_models(&LOBE_MODELS);
    let mut count = 0;
    for (file, material, text) in examples() {
        let location = file.display().to_string();
        let ir = shadewire_mtlx::to_ir(&text, Some(&location), Some(&material), &options, &Default::default())
            .unwrap_or_else(|e| panic!("{location} {material}: {e}"));
        let name = mdl_ident(&material);
        let out = compile(&ir.dag, &ir.source, &MdlOptions { name: name.clone(), ..Default::default() })
            .unwrap_or_else(|e| panic!("{location} {material}: {e}"));
        common::assert_mdlc(&out.code, &format!("m{count}_{name}"));
        count += 1;
    }
    assert!(count >= 54, "{count} materials");
}
