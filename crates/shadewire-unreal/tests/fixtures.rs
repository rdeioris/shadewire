//! Every fixture under unreal/fixtures/graphs (dumped by the plugin's commandlet from unreal/ShadewireTest) lowers,
//! its WGSL validates and its MDL is written; the warnings and the MaterialX are snapshots, and the MaterialX reads
//! back to the same IR.

use std::path::{Path, PathBuf};

use shadewire::shader::CompileOptions;
use shadewire::testing::assert_same_material;
use shadewire_unreal::{IrOptions, MaterialIr, to_ir};

fn fixtures() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../unreal/fixtures/graphs");
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

fn lower(name: &str, text: &str, mode: &str, options: &IrOptions) -> MaterialIr {
    to_ir(text, None, options).unwrap_or_else(|e| panic!("{name} ({mode}): {e}"))
}

#[test]
fn every_fixture_lowers_and_its_warnings_and_materialx_are_the_snapshots() {
    for (name, text) in fixtures() {
        let ir = lower(&name, &text, "models", &IrOptions::default());
        assert_eq!(ir.source.source, "unreal");
        insta::assert_snapshot!(format!("{name}_warnings"), ir.warnings.join("\n"));
        let write = shadewire_mtlx::WriteOptions { name: name.clone(), ..Default::default() };
        let written = shadewire_mtlx::from_ir(&ir.dag, &write).unwrap_or_else(|e| panic!("{name}: {e}"));
        insta::assert_snapshot!(format!("{name}_mtlx"), written.text);
    }
}

#[test]
fn every_fixture_validates_as_wgsl_in_both_modes() {
    let options = CompileOptions::default();
    for (name, text) in fixtures() {
        for (mode, lower_options) in modes() {
            let ir = lower(&name, &text, mode, &lower_options);
            let wgsl = shadewire_wgsl::compile(&ir.dag, &ir.source, &options).unwrap_or_else(|e| panic!("{name}: {e}"));
            shadewire_wgsl::validate::check(&wgsl.code, &wgsl.reflection)
                .unwrap_or_else(|e| panic!("{name} ({mode}): the WGSL does not validate:\n{e}"));
        }
    }
}

#[test]
fn every_fixture_writes_mdl_as_closure_trees() {
    let (_, lobes) = &modes()[1];
    for (name, text) in fixtures() {
        let ir = lower(&name, &text, "lobes", lobes);
        let options = shadewire_mdl::MdlOptions { name: name.clone(), ..Default::default() };
        let mdl = shadewire_mdl::compile(&ir.dag, &ir.source, &options).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(mdl.code.contains(&format!("export material {name}(")), "{name}:\n{}", mdl.code);
    }
}

#[test]
fn every_fixture_reads_back_from_its_materialx_as_the_same_ir() {
    for (name, text) in fixtures() {
        let ir = lower(&name, &text, "models", &IrOptions::default());
        let written = shadewire_mtlx::from_ir(&ir.dag, &Default::default()).unwrap();
        let back = shadewire_mtlx::to_ir(&written.text, None, None, &IrOptions::default(), &Default::default())
            .unwrap_or_else(|e| panic!("{name}: {e}\n{}", written.text));
        assert_same_material(&ir.dag, &back.dag, &written.text);
    }
}
